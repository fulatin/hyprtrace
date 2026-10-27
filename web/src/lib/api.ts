import { authHeaders, clearAuthToken } from './auth';
import type {
  TodaySummary,
  AppRank,
  HourlyBucket,
  Session,
  SearchHit,
  DailyTrend,
  AiMessage,
  AiModelsResponse,
  AiStatus,
  AiToolsResponse,
  ConfigResponse,
  ConfigUpdateRequest,
  PaginatedResponse,
  ActivityEvent,
  CategoryRule,
  CategoriesResponse,
  Project,
  ProjectRule,
  ProjectStat,
  ProjectsResponse,
  AppResource,
  DisruptionEvent,
  EfficiencyScore,
  Goal,
  GoalsResponse,
  TrendPrediction,
  AppsMetadataResponse,
  DailyActivity,
  TitleStat,
  TransitionsResponse,
  ForecastResponse,
  RhythmResponse,
  FragmentationResponse,
  CooccurrenceResponse,
  DisruptionImpactResponse,
  AnomaliesResponse,
} from './types';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const headers = new Headers(options?.headers);
  for (const [k, v] of Object.entries(authHeaders())) {
    headers.set(k, v);
  }
  // Timeout a hung request so the UI shows an error instead of staying in a
  // perpetual loading state. Callers that need a longer window (e.g. the AI
  // stream) pass their own signal, which takes precedence.
  const signal = options?.signal ?? AbortSignal.timeout(15_000);
  const res = await fetch(url, { ...options, headers, signal });
  if (res.status === 401) {
    // The token was rejected (e.g. the server's auth_token changed or the
    // saved token is stale). Clear it so the next request doesn't keep failing
    // with a secret that is no longer valid.
    clearAuthToken();
  }
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`API Error: ${res.status} ${text}`);
  }
  return res.json();
}

/**
 * Extract a human-readable message from an API error thrown by fetchJSON,
 * which formats failures as "API Error: <status> <body>".
 */
export function extractApiError(e: unknown): string {
  if (e instanceof Error) {
    const idx = e.message.indexOf('{');
    if (idx >= 0) {
      try {
        const parsed = JSON.parse(e.message.slice(idx));
        if (parsed && typeof parsed.error === 'string') {
          return parsed.error;
        }
      } catch {
        /* fall through */
      }
    }
    return e.message;
  }
  return 'Unknown error';
}

export const api = {
  health: () =>
    fetchJSON<{ status: string; version: string }>('/api/health'),

  summary: (date: string) =>
    fetchJSON<TodaySummary>(`/api/summary?date=${encodeURIComponent(date)}`),

  appRanking: (from: string, to: string, limit = 10) =>
    fetchJSON<AppRank[]>(
      `/api/apps?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
    ),

  timeline: (date: string) =>
    fetchJSON<HourlyBucket[]>(`/api/timeline?date=${encodeURIComponent(date)}`),

  sessions: (from: string, to: string, page = 1, perPage = 50, cls?: string) =>
    fetchJSON<PaginatedResponse<Session>>(
      `/api/sessions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&page=${page}&per_page=${perPage}${cls ? `&class=${encodeURIComponent(cls)}` : ''}`
    ),

  // Full-text search over window titles. Unlike the other list calls there is
  // no default date window: an empty `from`/`to` searches the whole history,
  // which is what "when did I last look at this" usually means.
  search: (q: string, from = '', to = '', limit = 100) =>
    fetchJSON<SearchHit[]>(
      `/api/search?q=${encodeURIComponent(q)}&from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
    ),

  appClasses: (from: string, to: string) =>
    fetchJSON<string[]>(`/api/apps/classes?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`),
  appsMetadata: (classes: string[]) =>
    fetchJSON<AppsMetadataResponse>(
      `/api/apps/metadata?classes=${encodeURIComponent(classes.join(','))}`
    ),

  appTrend: (cls: string, from: string, to: string, granularity?: string) =>
    fetchJSON<DailyTrend[]>(
      `/api/app/${encodeURIComponent(cls)}/trend?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}${granularity ? `&granularity=${granularity}` : ''}`
    ),

  /**
   * GET /api/ai/models — model inventory straight from the harness. Without
   * `refresh` this never boots the harness (a page load must not pay a
   * multi-second child start); `refresh=1` starts it if needed, which is the
   * Settings page's "test connection" action.
   */
  aiModels: (refresh = false) =>
    fetchJSON<AiModelsResponse>(
      `/api/ai/models${refresh ? '?refresh=1' : ''}`,
      // `refresh=1` may have to boot the harness (a child process), which can
      // take well over the default 15s on a cold machine.
      refresh ? { signal: AbortSignal.timeout(60_000) } : undefined
    ),

  /** GET /api/ai/status — DSH runtime health (enabled/ready/bridge/tool count). */
  aiStatus: () =>
    fetchJSON<AiStatus>('/api/ai/status'),

  /** GET /api/ai/tools — tools the harness actually exposes this session. */
  aiTools: () =>
    fetchJSON<AiToolsResponse>('/api/ai/tools'),

  /**
   * POST /api/ai/cancel — ask the server to cancel the in-flight turn of a
   * conversation. Aborting the NDJSON fetch is the primary stop path (the
   * server treats a disconnect as a cancel); this is the explicit backup.
   */
  aiCancel: (conversationId: string) =>
    fetchJSON<{ status: 'ok' | 'not_found' }>('/api/ai/cancel', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ conversation_id: conversationId }),
      // The default 15s timeout is fine here; the call answers immediately.
    }),

  aiConversations: () =>
    fetchJSON<AiMessage[]>('/api/ai/conversations'),

  clearConversations: () =>
    fetchJSON<{ status: string }>('/api/ai/conversations', { method: 'DELETE' }),

  getConfig: () =>
    fetchJSON<ConfigResponse>('/api/config'),

  updateConfig: (req: ConfigUpdateRequest) =>
    fetchJSON<{ status: string }>('/api/config', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    }),

  deleteSessions: (from: string, to: string, cls?: string) =>
    fetchJSON<{ deleted: number; rebuilt_summaries: boolean }>(
      `/api/sessions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}${cls ? `&class=${encodeURIComponent(cls)}` : ''}`,
      { method: 'DELETE' }
    ),

  activityEvents: (from: string, to: string, limit = 100) =>
    fetchJSON<ActivityEvent[]>(
      `/api/activity/events?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
    ),

  activityDaily: (days = 371) =>
    fetchJSON<DailyActivity[]>(`/api/activity/daily?days=${days}`),

  titles: (from: string, to: string, cls?: string, limit = 100) =>
    fetchJSON<TitleStat[]>(
      `/api/titles?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}${cls ? `&class=${encodeURIComponent(cls)}` : ''}`
    ),

  rebuildHourlySummary: () =>
    fetchJSON<{ status: string }>('/api/hourly-summary/rebuild', { method: 'POST' }),

  categories: () =>
    fetchJSON<CategoriesResponse>('/api/categories'),

  putCategories: (rules: CategoryRule[]) =>
    fetchJSON<{ status: string }>('/api/categories', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ rules }),
    }),

  projects: () =>
    fetchJSON<ProjectsResponse>('/api/projects'),

  putProjects: (projects: Project[], rules: ProjectRule[]) =>
    fetchJSON<{ status: string; projects: Project[]; rules: ProjectRule[] }>('/api/projects', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ projects, rules }),
    }),

  projectStats: (from: string, to: string) =>
    fetchJSON<ProjectStat[]>(
      `/api/projects/stats?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
    ),

  resources: (from: string, to: string, limit = 10) =>
    fetchJSON<AppResource[]>(
      `/api/resources?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
    ),

  disruptions: (from: string, to: string, limit = 50) =>
    fetchJSON<DisruptionEvent[]>(
      `/api/disruptions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
    ),

  efficiency: (date: string) =>
    fetchJSON<EfficiencyScore>(`/api/efficiency?date=${encodeURIComponent(date)}`),

  goals: () =>
    fetchJSON<GoalsResponse>('/api/goals'),

  putGoals: (goals: Goal[]) =>
    fetchJSON<{ status: string }>('/api/goals', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ goals }),
    }),

  predict: (window = 14) =>
    fetchJSON<TrendPrediction>(`/api/predict?window=${window}`),

  /* ------------------------------------------------------------------------
     Insights: deeper analysis endpoints (Markov transitions, regression
     forecasts, rhythm, fragmentation, co-occurrence, disruption impact,
     anomaly detection).
     ---------------------------------------------------------------------- */
  insights: {
    transitions: (from: string, to: string, minMs = 3000, limit = 12) =>
      fetchJSON<TransitionsResponse>(
        `/api/insights/transitions?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&min_ms=${minMs}&limit=${limit}`
      ),

    forecast: (cls?: string, days = 30, horizon = 7) =>
      fetchJSON<ForecastResponse>(
        `/api/insights/forecast?class=${encodeURIComponent(cls ?? '')}&days=${days}&horizon=${horizon}`
      ),

    rhythm: (from: string, to: string) =>
      fetchJSON<RhythmResponse>(
        `/api/insights/rhythm?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      ),

    fragmentation: (from: string, to: string) =>
      fetchJSON<FragmentationResponse>(
        `/api/insights/fragmentation?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      ),

    cooccurrence: (from: string, to: string, windowMinutes = 30, limit = 10) =>
      fetchJSON<CooccurrenceResponse>(
        `/api/insights/cooccurrence?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&window_minutes=${windowMinutes}&limit=${limit}`
      ),

    disruptionImpact: (from: string, to: string) =>
      fetchJSON<DisruptionImpactResponse>(
        `/api/insights/disruption-impact?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      ),

    anomalies: (from: string, to: string, window = 14, threshold = 2) =>
      fetchJSON<AnomaliesResponse>(
        `/api/insights/anomalies?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&window=${window}&threshold=${threshold}`
      ),
  },

  report: async (from: string, to: string): Promise<void> => {
    const res = await fetch(`/api/report?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`, {
      headers: authHeaders(),
    });
    if (!res.ok) throw new Error(`API Error: ${res.status}`);
    const md = await res.text();
    const blob = new Blob([md], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `hyprtrace-report-${from}-${to}.md`;
    a.click();
    URL.revokeObjectURL(url);
  },

  /**
   * POST /api/ai/report/weekly — server-side weekly report written by DSH.
   * The `model` override is optional; the response is the Markdown text.
   */
  weeklyReport: (model?: string) =>
    fetchJSON<{ report: string }>('/api/ai/report/weekly', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(model ? { model } : {}),
      // The report is a full model turn (often ~30-60s), not a quick API read.
      signal: AbortSignal.timeout(120_000),
    }),
};