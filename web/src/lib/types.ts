// The model types in this file are GENERATED from the Rust definitions — do not
// write them by hand. `crates/hyprtrace-server/src/api_types.rs` renders
// `./api-types.ts`, and a Rust test fails when the two disagree, so a manual
// edit here only moves the failure to CI.
//
// This module re-exports them under the names the app already imports and adds
// the shapes that exist only on the client side (request bodies and the
// response wrappers around a generic payload).
import type {
  AppMetadata,
  CategoryRule,
  Goal,
  GoalProgress,
  Project,
  ProjectRule,
} from './api-types';

export type {
  ActivityEvent,
  AiMessage,
  AppMetadata,
  AppRank,
  AppResource,
  CategoryRule,
  CurrentStatus,
  DailyActivity,
  DailyTrend,
  DisruptionEvent,
  EfficiencyScore,
  Goal,
  GoalProgress,
  HourlyBucket,
  Project,
  ProjectRule,
  ProjectStat,
  SearchHit,
  Session,
  TitleStat,
  TodaySummary,
  TrendPrediction,
  WorkspaceRecommendation,
} from './api-types';

export interface CategoriesResponse {
  rules: CategoryRule[];
  categories: string[];
}

export interface ProjectsResponse {
  projects: Project[];
  rules: ProjectRule[];
}

export interface GoalsResponse {
  goals: Goal[];
  progress: GoalProgress[];
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
