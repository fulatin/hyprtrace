import type {
  TodaySummary,
  AppRank,
  HourlyBucket,
  Session,
  DailyTrend,
  AiMessage,
  AiChatResponse,
  AiModelsResponse,
  ConfigResponse,
  ConfigUpdateRequest,
  PaginatedResponse,
  ActivityEvent,
  CategoryRule,
  CategoriesResponse,
} from './types';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const res = await fetch(url, options);
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

  appTrend: (cls: string, from: string, to: string, granularity?: string) =>
    fetchJSON<DailyTrend[]>(
      `/api/app/${encodeURIComponent(cls)}/trend?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}${granularity ? `&granularity=${granularity}` : ''}`
    ),

  aiModels: () =>
    fetchJSON<AiModelsResponse>('/api/ai/models'),

  aiChat: (provider: string, message: string, includeData: boolean, dateRange: string) =>
    fetchJSON<AiChatResponse>('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider,
        message,
        include_data: includeData,
        date_range: dateRange,
      }),
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

  activityEvents: (from: string, to: string, limit = 100) =>
    fetchJSON<ActivityEvent[]>(
      `/api/activity/events?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&limit=${limit}`
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
};