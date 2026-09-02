import type { RouteAttempt, RunSummary, SourceDiagnostic } from "./contracts";

export type HealthPanel = "unstable" | "failed";

export interface SourceHealthDetail {
  source: string;
  status: HealthPanel;
  host: string;
  url: string;
  failureClass: string;
  failureLabel: string;
  message: string;
  retryCount: number;
  usedFallback: boolean;
  finalRouteId: string;
  finalHost: string;
  finalUrl: string;
  itemCount: number;
  elapsedSeconds: number;
  attempts: RouteAttempt[];
}

const FAILURE_LABELS: Record<string, string> = {
  source_outage: "來源暫時故障",
  runner_network: "網路連線失敗",
  tls_certificate: "TLS 憑證錯誤",
  access_blocked: "網站拒絕存取",
  parser_regression: "頁面解析失敗",
  browser_runtime: "瀏覽器執行失敗",
  unknown: "未知來源錯誤",
};

export function failureClassLabel(failureClass: string): string {
  return FAILURE_LABELS[failureClass] ?? (failureClass || "未提供失敗分類");
}

function latestAttempt(attempts: RouteAttempt[], status: RouteAttempt["status"]): RouteAttempt | undefined {
  return [...attempts].reverse().find((attempt) => attempt.status === status);
}

function countRetries(attempts: RouteAttempt[]): number {
  const seenRoutes = new Set<string>();
  let retries = 0;
  for (const attempt of attempts) {
    const route = attempt.route_id ?? attempt.url ?? "unknown";
    if ((attempt.attempt_number ?? 1) > 1 || seenRoutes.has(route)) retries += 1;
    seenRoutes.add(route);
  }
  return retries;
}

function detailFromDiagnostic(
  diagnostic: SourceDiagnostic,
  attempts: RouteAttempt[],
  status: HealthPanel,
): SourceHealthDetail {
  const latestFailure = latestAttempt(attempts, "failed");
  const latestSuccess = latestAttempt(attempts, "success");
  const finalRoute = diagnostic.final_route ?? {};
  const evidence = diagnostic.failure_evidence ?? latestFailure?.failure_evidence ?? {};
  const failureClass =
    diagnostic.failure_class || diagnostic.last_failure_class || latestFailure?.failure_class || "unknown";
  const host = evidence.url_host || latestFailure?.url_host || finalRoute.url_host || latestSuccess?.url_host || "";
  const url = latestFailure?.url || finalRoute.url || latestSuccess?.url || "";

  return {
    source: diagnostic.source,
    status,
    host,
    url,
    failureClass,
    failureLabel: failureClassLabel(failureClass),
    message: evidence.message ?? "報告未提供錯誤訊息",
    retryCount: countRetries(attempts),
    usedFallback: Boolean(finalRoute.used_fallback),
    finalRouteId: finalRoute.route_id ?? latestSuccess?.route_id ?? "",
    finalHost: finalRoute.url_host ?? latestSuccess?.url_host ?? "",
    finalUrl: finalRoute.url ?? latestSuccess?.url ?? "",
    itemCount: diagnostic.item_count ?? latestSuccess?.item_count ?? 0,
    elapsedSeconds: diagnostic.elapsed_seconds ?? attempts.reduce((total, attempt) => total + (attempt.elapsed_seconds ?? 0), 0),
    attempts,
  };
}

export function buildSourceHealthDetails(summary: RunSummary, panel: HealthPanel): SourceHealthDetail[] {
  const diagnostics = Array.isArray(summary.source_diagnostics) ? summary.source_diagnostics : [];
  const routeAttempts = Array.isArray(summary.route_attempts) ? summary.route_attempts : [];
  const matching = diagnostics.filter((diagnostic) =>
    panel === "failed" ? diagnostic.status === "failed" : diagnostic.status === "success" && diagnostic.unstable === true,
  );
  const details = matching.map((diagnostic) =>
    detailFromDiagnostic(
      diagnostic,
      routeAttempts.filter((attempt) => attempt.source === diagnostic.source),
      panel,
    ),
  );

  if (panel === "failed") {
    const known = new Set(details.map((detail) => detail.source));
    for (const source of summary.failed_sources ?? []) {
      if (!known.has(source)) {
        details.push(
          detailFromDiagnostic({ source, status: "failed" }, [], "failed"),
        );
      }
    }
  }

  return details.sort((left, right) => left.source.localeCompare(right.source, "zh-Hant"));
}

export function healthTooltip(details: SourceHealthDetail[]): string[] {
  return details.map((detail) => `${detail.source}${detail.host ? ` · ${detail.host}` : ""}`);
}

export function toggleHealthPanel(
  current: HealthPanel | null,
  selected: HealthPanel,
  detailCount: number,
): HealthPanel | null {
  if (detailCount === 0) return current;
  return current === selected ? null : selected;
}
