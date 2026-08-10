export type RunStatus = "success" | "attention" | "partial_failure" | "failure";

export interface SourceHealth {
  healthy_count: number;
  unstable_count: number;
  failed_count: number;
  fallback_source_count: number;
  coverage_reduced_count?: number;
  ssl_fallback_host_count?: number;
}

export interface QualitySummary {
  input_count?: number;
  output_count?: number;
  duplicate_count?: number;
  invalid_count?: number;
  excluded_non_news_count?: number;
  summary_coverage_rate?: number;
  alert_reasons?: string[];
  source_counts?: Record<string, number>;
}

export interface RunSummary {
  status: RunStatus;
  report_schema_version?: number;
  started_at?: string;
  finished_at?: string;
  duration_seconds?: number;
  selected_source_count?: number;
  selected_sources?: string[];
  news_count: number;
  failed_sources: string[];
  anomalies: string[];
  failure_class_counts: Record<string, number>;
  error_counts?: Record<string, number>;
  parser_warnings?: unknown[];
  scheduling_plan?: unknown[];
  alerts?: unknown[];
  source_attempts?: unknown[];
  source_diagnostics?: unknown[];
  route_attempts?: unknown[];
  insecure_ssl_hosts?: string[];
  source_health: SourceHealth;
  quality: QualitySummary;
  relevance_policy: Record<string, unknown>;
  output_file: string;
  report_file: string;
  week_start: string;
  week_end: string;
  error?: string;
}

export interface RunOptions {
  sources: string[];
  date?: string;
  start_date?: string;
  end_date?: string;
  output_dir?: string;
  report_dir?: string;
  max_workers: number;
  dedupe_affiliated: boolean;
  fail_on_source_error: boolean;
}

export interface ProgressEvent {
  kind: "started" | "source_started" | "source_finished" | "source_failed" | "retry" | "writing_outputs" | "cancelling" | "cancelled" | "completed" | "failed";
  source?: string;
  completed?: number;
  total?: number;
  message?: string;
}
