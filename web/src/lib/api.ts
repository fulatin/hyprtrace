import { authHeaders } from './auth';
import type {
  TodaySummary,
  AppRank,
  HourlyBucket,
  Session,
  DailyTrend,
  AiMessage,
  AiModelsResponse,
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
} from './types';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const headers = new Headers(options?.headers);
  for (const [k, v] of Object.entries(authHeaders())) {
    headers.set(k, v);
  }
  const res = await fetch(url, { ...options, headers });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`API Error: ${res.status} ${text}`);
  }
  return res.json();
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

  aiModels: () =>
    fetchJSON<AiModelsResponse>('/api/ai/models'),



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

  weeklyReport: (provider: string, model?: string) =>
    fetchJSON<{ report: string }>('/api/ai/report/weekly', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ provider, model }),
    }),
};