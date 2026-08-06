export type RunStatus = "success" | "attention" | "partial_failure" | "failure";

export interface SourceHealth {
  healthy_count: number;
  unstable_count: number;
  failed_count: number;
  fallback_source_count: number;
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
  news_count: number;
  failed_sources: string[];
  anomalies: string[];
  failure_class_counts: Record<string, number>;
  source_health: SourceHealth;
  quality: QualitySummary;
  relevance_policy: Record<string, unknown>;
  output_file: string;
  report_file: string;
  error?: string;
}

export interface RunOptions {
  sources: string[];
  output_dir?: string;
  report_dir?: string;
  max_workers: number;
  dedupe_affiliated: boolean;
  fail_on_source_error: boolean;
}

export interface ProgressEvent {
  kind: "started" | "source_started" | "source_finished" | "retry" | "completed" | "failed";
  source?: string;
  completed?: number;
  total?: number;
  message?: string;
}
