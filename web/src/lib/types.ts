export interface TodaySummary {
  date: string;
  total_active_ms: number;
  total_idle_ms: number;
  total_focused_ms: number;
  app_count: number;
  session_count: number;
  top_apps: AppRank[];
}

export interface AppRank {
  class: string;
  total_ms: number;
  percentage: number;
  session_count: number;
  focused_ms: number;
  focused_session_count: number;
  category?: string;
}

export interface CategoryRule {
  id?: number | null;
  pattern: string;
  category: string;
  priority?: number;
}

export interface CategoriesResponse {
  rules: CategoryRule[];
  categories: string[];
}

export interface Project {
  id?: number | null;
  name: string;
  color: string;
  sort_order: number;
}

export interface ProjectRule {
  id?: number | null;
  project_id: number;
  pattern: string;
  priority: number;
}

export interface ProjectStat {
  project_id: number | null;
  name: string;
  color: string;
  total_ms: number;
  session_count: number;
  percentage: number;
}

export interface ProjectsResponse {
  projects: Project[];
  rules: ProjectRule[];
}

export interface AppResource {
  class: string;
  avg_cpu_pct: number;
  peak_mem_kb: number;
  sample_count: number;
}

export interface DisruptionEvent {
  id: number;
  kind: string;
  app: string | null;
  summary: string | null;
  occurred_at: string;
}

export interface EfficiencyScore {
  date: string;
  score: number;
  focus_ratio: number;
  avg_session_secs: number;
  late_night_pct: number;
  disruption_count: number;
  total_active_ms: number;
}

export interface Goal {
  id?: number | null;
  name: string;
  target_type: string;
  target_key?: string | null;
  daily_target_ms: number;
  enabled: boolean;
}

export interface GoalProgress {
  goal: Goal;
  today_ms: number;
  pct: number;
}

export interface GoalsResponse {
  goals: Goal[];
  progress: GoalProgress[];
}

export interface TrendPrediction {
  today_ms: number;
  predicted_today_ms: number;
  predicted_tomorrow_ms: number;
  daily_avg_ms: number;
  slope: number;
  window_days: number;
}

export interface HourlyBucket {
  hour: number;
  total_ms: number;
  session_count: number;
  focused_ms: number;
}

export interface Session {
  id: number;
  class: string;
  title: string;
  workspace: string | null;
  started_at: string;
  ended_at: string | null;
  duration_ms: number | null;
  activity_state: string | null;
  focused_ms: number | null;
}

export interface DailyTrend {
  date: string;
  total_ms: number;
  session_count: number;
  focused_ms: number;
}

export interface AiMessage {
  id: number;
  created_at: string;
  role: string;
  content: string;
  model: string;
  complete: boolean | null;
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
  retention_days: number;
}

export interface ConfigUpdateRequest {
  openai_url?: string;
  openai_api_key?: string;
  openai_model?: string;
  ollama_url?: string;
  ollama_model?: string;
  default_provider?: string;
  retention_days?: number;
}

export interface ActivityEvent {
  id: number;
  session_id: number | null;
  state: string;
  started_at: string;
  ended_at: string | null;
  duration_ms: number | null;
}

export interface AppMetadata {
  desktop_id: string;
  display_name: string;
  icon: string;
}

export interface AppsMetadataResponse {
  entries: Record<string, AppMetadata>;
}
