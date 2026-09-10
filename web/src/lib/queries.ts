import { QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api } from './api';
import type { CategoryRule, ConfigUpdateRequest, Goal, Project, ProjectRule, Session } from './types';

/**
 * Query keys used by the Settings sections.
 *
 * Every key is ONE shared cache entry. The AI, weekly-report and privacy
 * sections each own different keys of the same `/api/config` object, so they
 * all read and write the single `['config']` key: a save in one section has to
 * be visible in another, which is exactly what the page used to get from
 * holding one `config` object in the parent.
 */
export const queryKeys = {
  health: ['health'] as const,
  aiModels: ['ai-models'] as const,
  config: ['config'] as const,
  categories: ['categories'] as const,
  goals: ['goals'] as const,
  projects: ['projects'] as const,
};

/**
 * Defaults mirror what the page did before the split: each request fired once
 * per mount and was never retried.
 *
 * - `retry: false` — a failing API is not hammered; a failed fetch leaves the
 *   section empty, which is what the old `.catch(() => {})` chain did.
 * - `staleTime: 30s` — re-entering the route within half a minute does not
 *   refetch every field on the page.
 * - no focus/reconnect refetch — the old page only fetched on mount and after
 *   an explicit save, so a tab switch must not silently replace the form.
 *
 * Saves still read fresh data: every mutation invalidates the key it wrote.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      staleTime: 30_000,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
    },
    mutations: { retry: false },
  },
});

/* --------------------------------------------------------------------------
   Queries
   -------------------------------------------------------------------------- */

/** GET /api/health — drives the Server panel's online/offline indicator. */
export function useHealth() {
  return useQuery({ queryKey: queryKeys.health, queryFn: api.health });
}

/** GET /api/ai/models — provider list for the AI providers panel. */
export function useAiModels() {
  return useQuery({ queryKey: queryKeys.aiModels, queryFn: api.aiModels });
}

/** GET /api/config — one entry shared by the AI, weekly-report and privacy sections. */
export function useConfig() {
  return useQuery({ queryKey: queryKeys.config, queryFn: api.getConfig });
}

/** GET /api/categories — category rules plus the known category names. */
export function useCategories() {
  return useQuery({ queryKey: queryKeys.categories, queryFn: api.categories });
}

/** GET /api/goals — daily goals only; `progress` is not rendered here. */
export function useGoals() {
  return useQuery({ queryKey: queryKeys.goals, queryFn: api.goals });
}

/** GET /api/projects — projects plus their matching rules. */
export function useProjects() {
  return useQuery({ queryKey: queryKeys.projects, queryFn: api.projects });
}

/* --------------------------------------------------------------------------
   Mutations

   PUT /api/config is a PARTIAL update: the server merges only the keys present
   in the request, so each section sends its own slice and no section can clobber
   another's values. Every config mutation invalidates the shared `['config']`
   key, so the sections that render config fields straight from the cache (the
   OpenAI "Configured" chip) see the union of what everyone wrote.
   -------------------------------------------------------------------------- */

export function useUpdateConfig() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (req: ConfigUpdateRequest) => api.updateConfig(req),
    onSuccess: () => client.invalidateQueries({ queryKey: queryKeys.config }),
  });
}

export function useSaveCategories() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (rules: CategoryRule[]) => api.putCategories(rules),
    // PUT /api/categories answers with a bare status, so the list has to be
    // re-read — the old handler did the same GET right after the write, and the
    // section's draft is re-seeded from it (server-assigned ids).
    onSuccess: () => client.invalidateQueries({ queryKey: queryKeys.categories }),
  });
}

export function useSaveGoals() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (goals: Goal[]) => api.putGoals(goals),
    // Stale, but no refetch: the old handler kept the edited draft on screen
    // (PUT /api/goals answers with a bare status). Refetching here would re-key
    // every unsaved row and replay its entrance animation.
    onSuccess: () => client.invalidateQueries({ queryKey: queryKeys.goals, refetchType: 'none' }),
  });
}

/**
 * Persist the projects draft and its rules.
 *
 * The two-phase dance (used only when a rule points at a project that has no id
 * yet) lives here rather than in the section so that the cache is written ONCE,
 * with the final response: the intermediate projects-only response must not
 * reach the cache, or the section's draft would be re-seeded mid-save and the
 * rule rows would flash out and back in.
 */
export function useSaveProjects() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({
      projects,
      rules,
    }: {
      projects: Project[];
      rules: ProjectRule[];
    }): Promise<{ status: string; projects: Project[]; rules: ProjectRule[] }> => {
      const projs = projects.map((p, i) => ({ ...p, sort_order: i })).filter((p) => p.name.trim());
      const keptRules = rules.filter((r) => r.pattern.trim());

      if (!keptRules.some((r) => r.project_id < 0)) {
        return api.putProjects(projs, keptRules);
      }

      // Persist the projects first to obtain their real ids.
      const first = await api.putProjects(projs, []);
      const nameToId = new Map<string, number>();
      first.projects.forEach((p) => {
        if (p.id != null) nameToId.set(p.name, p.id);
      });

      // Resolve every rule's project to the newly-saved id. Negative temp ids
      // are an index into the draft (`projects`): -1 → projects[0], ...;
      // existing ids are looked up by name because a full replace renumbers rows.
      const finalRules = keptRules.map((r) => {
        let ref: Project | undefined;
        if (r.project_id < 0) {
          const idx = -r.project_id - 1;
          ref = idx >= 0 && idx < projects.length ? projects[idx] : undefined;
        } else {
          ref = projects.find((p) => p.id === r.project_id);
        }
        // Prefer the freshly-saved id (by name); fall back to any id we already
        // have, else leave the rule's id as-is for the server to drop.
        const realId = ref ? nameToId.get(ref.name) ?? ref.id : undefined;
        return { ...r, project_id: realId ?? r.project_id };
      });

      // Save again with the persisted projects (carrying real ids) so the
      // remapped rules resolve against the projects that actually exist.
      return api.putProjects(first.projects, finalRules);
    },
    // PUT /api/projects answers with the saved projects and rules — the cached
    // shape exactly — so the cache is refreshed from the response instead of a
    // second round trip. The old handler used the same response.
    onSuccess: (saved) =>
      client.setQueryData(queryKeys.projects, { projects: saved.projects, rules: saved.rules }),
  });
}

/** POST /api/hourly-summary/rebuild — no cache effect, the summary is not read here. */
export function useRebuildHourlySummary() {
  return useMutation({ mutationFn: () => api.rebuildHourlySummary() });
}

/**
 * DELETE /api/sessions — nothing on this page is derived from sessions, so no
 * key is invalidated (the old handler refreshed nothing either).
 */
export function useDeleteSessions() {
  return useMutation({
    mutationFn: ({ from, to, cls }: { from: string; to: string; cls?: string }) =>
      api.deleteSessions(from, to, cls),
  });
}

/**
 * The CSV export walks every page of `/api/sessions` in a date range: a read,
 * but an imperative one with no cacheable identity, so it is a mutation that
 * resolves to the full session list — the caller builds the CSV from it.
 */
export function useExportSessions() {
  return useMutation({
    mutationFn: async ({ from, to }: { from: string; to: string }): Promise<Session[]> => {
      let allSessions: Session[] = [];
      let page = 1;
      const perPage = 200;
      let totalFetched = 0;

      while (true) {
        const res = await api.sessions(from, to, page, perPage);
        allSessions = allSessions.concat(res.data);
        totalFetched += res.data.length;
        if (totalFetched >= res.total || res.data.length === 0) break;
        page++;
      }

      return allSessions;
    },
  });
}

/** GET /api/report — the server answers with the Markdown download itself. */
export function useDownloadWeeklyReport() {
  return useMutation({
    mutationFn: ({ from, to }: { from: string; to: string }) => api.report(from, to),
  });
}
