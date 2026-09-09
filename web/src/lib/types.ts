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

export interface DailyActivity {
  date: string;
  total_ms: number;
  focused_ms: number;
  session_count: number;
}

export interface TitleStat {
  class: string;
  title: string;
  total_ms: number;
  session_count: number;
  last_used_at: string;
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
  record_titles: boolean;
  retention_days: number;
  weekly_report_enabled: boolean;
  weekly_report_day: number;
  weekly_report_hour: number;
  weekly_report_minute: number
}

export interface ConfigUpdateRequest {
  openai_url?: string;
  openai_api_key?: string;
  openai_model?: string;
  ollama_url?: string;
  ollama_model?: string;
  default_provider?: string;
  record_titles?: boolean;
  retention_days?: number;
  weekly_report_enabled?: boolean;
  weekly_report_day?: number;
  weekly_report_hour?: number;
  weekly_report_minute?: number
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

/* --------------------------------------------------------------------------
   Insights API (/api/insights/*)
   -------------------------------------------------------------------------- */

/** One app node in the transition graph. */
export interface TransitionNode {
  class: string;
  total_ms: number;
  session_count: number;
  /** Share of all tracked time in the window (0..1). */
  share: number;
  /** Number of outgoing transitions (including self). */
  out_total: number;
  /** P(next session is the same app). */
  self_probability: number;
  avg_dwell_ms: number;
}

/** One A→B transition edge. `probability` is row-normalised over A's outgoing. */
export interface TransitionEdge {
  from: string;
  to: string;
  count: number;
  probability: number;
  from_out_total: number;
  avg_gap_ms: number;
}

export interface TransitionMatrix {
  classes: string[];
  /** values[i][j] = P(next = classes[j] | current = classes[i]) */
  values: number[][];
}

export interface TransitionFlow {
  from: string;
  to: string;
  count: number;
  probability: number;
}

export interface TransitionsResponse {
  from: string;
  to: string;
  min_ms: number;
  limit: number;
  total_transitions: number;
  total_sessions: number;
  nodes: TransitionNode[];
  edges: TransitionEdge[];
  matrix: TransitionMatrix;
  top_flows: TransitionFlow[];
}

export interface ForecastPoint {
  date: string;
  /** Absent on future points. */
  actual_ms?: number;
  predicted_ms: number;
  lower_ms: number;
  upper_ms: number;
}

export interface ForecastResponse {
  /** null when the forecast is for total tracked time across all apps. */
  class: string | null;
  days: number;
  horizon: number;
  slope_ms_per_day: number;
  intercept_ms: number;
  r2: number;
  mae_ms: number;
  residual_std_ms: number;
  avg_ms: number;
  trend: 'up' | 'down' | 'flat';
  today_ms: number;
  today_projected_ms: number;
  tomorrow_projected_ms: number;
  /** Multiplier per weekday, index 0 = Monday. */
  weekday_factors: number[];
  points: ForecastPoint[];
  forecast: ForecastPoint[];
}

export interface RhythmCell {
  /** 0 = Monday … 6 = Sunday. */
  weekday: number;
  hour: number;
  total_ms: number;
  /** Average per matching weekday in the window. */
  avg_ms: number;
  days: number;
}

export interface RhythmResponse {
  from: string;
  to: string;
  max_avg_ms: number;
  cells: RhythmCell[];
}

export interface FragmentationDay {
  date: string;
  session_count: number;
  switch_count: number;
  avg_dwell_ms: number;
  median_dwell_ms: number;
  /** Share of sessions shorter than a minute. */
  short_session_ratio: number;
  longest_focus_ms: number;
  focus_blocks: number;
  active_ms: number;
  context_switch_cost_ms: number;
}

export interface FragmentationResponse {
  from: string;
  to: string;
  days: FragmentationDay[];
  avg_dwell_ms: number;
  median_dwell_ms: number;
  avg_switch_count: number;
  short_session_ratio: number;
  switches_per_hour: number;
  trend_slope_ms_per_day: number;
  worst_day: { date: string; switch_count: number; avg_dwell_ms: number } | null;
}

export interface CooccurrencePair {
  a: string;
  b: string;
  co_occurrences: number;
  support: number;
  lift: number;
  jaccard: number;
  avg_gap_ms: number;
}

export interface CooccurrenceResponse {
  from: string;
  to: string;
  window_minutes: number;
  limit: number;
  pairs: CooccurrencePair[];
  nodes: { class: string; degree: number; total_ms: number }[];
}

export interface DisruptionDay {
  date: string;
  disruptions: number;
  notifications: number;
  clipboard: number;
  efficiency: number;
  focus_ratio: number;
  active_ms: number;
  avg_dwell_ms: number;
  session_count: number;
}

export interface DisruptionImpactResponse {
  from: string;
  to: string;
  n_days: number;
  days: DisruptionDay[];
  correlations: {
    efficiency_vs_disruptions: number;
    focus_ratio_vs_disruptions: number;
    avg_dwell_vs_disruptions: number;
    active_vs_disruptions: number;
  };
  high_disruption_days: { count: number; avg_efficiency: number; avg_focus_ratio: number };
  low_disruption_days: { count: number; avg_efficiency: number; avg_focus_ratio: number };
  insight: string;
}

export interface AnomalyDay {
  date: string;
  active_ms: number;
  baseline_ms: number;
  z_score: number;
  severity: 'low' | 'medium' | 'high';
  reasons: string[];
  late_night_share: number;
  session_count: number;
  avg_dwell_ms: number;
  weekday: number;
}

export interface AnomaliesResponse {
  from: string;
  to: string;
  window: number;
  threshold: number;
  baseline_days: number;
  days: AnomalyDay[];
}
