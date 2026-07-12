export interface TodaySummary {
  date: string;
  total_active_ms: number;
  total_idle_ms: number;
  app_count: number;
  session_count: number;
  top_apps: AppRank[];
}

export interface AppRank {
  class: string;
  total_ms: number;
  percentage: number;
  session_count: number;
}

export interface HourlyBucket {
  hour: number;
  total_ms: number;
  session_count: number;
}

export interface Session {
  id: number;
  class: string;
  title: string;
  workspace: string | null;
  started_at: string;
  ended_at: string | null;
  duration_ms: number | null;
}

export interface DailyTrend {
  date: string;
  total_ms: number;
  session_count: number;
}

export interface AiMessage {
  id: number;
  created_at: string;
  role: string;
  content: string;
  model: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  per_page: number;
}

export interface AiChatRequest {
  provider?: string;
  message: string;
  include_data?: boolean;
  date_range?: string;
}

export interface AiChatResponse {
  reply: string;
  model: string;
}

export interface AiModelsResponse {
  providers: Record<string, string[]>;
  default: string;
}

export interface ConfigResponse {
  openai_url: string;
  openai_model: string;
  openai_configured: boolean;
  ollama_url: string;
  ollama_model: string;
  default_provider: string;
}

export interface ConfigUpdateRequest {
  openai_url?: string;
  openai_api_key?: string;
  openai_model?: string;
  ollama_url?: string;
  ollama_model?: string;
  default_provider?: string;
}