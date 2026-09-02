import { describe, expect, it } from "vitest";
import type { RunSummary } from "./contracts";
import {
  buildSourceHealthDetails,
  failureClassLabel,
  healthTooltip,
  toggleHealthPanel,
} from "./source-health";

function summary(overrides: Partial<RunSummary>): RunSummary {
  return {
    status: "partial_failure",
    news_count: 0,
    failed_sources: [],
    anomalies: [],
    failure_class_counts: {},
    source_health: {
      healthy_count: 70,
      unstable_count: 1,
      failed_count: 1,
      fallback_source_count: 1,
    },
    quality: {},
    relevance_policy: {},
    output_file: "",
    report_file: "",
    week_start: "2026-08-31",
    week_end: "2026-09-06",
    ...overrides,
  };
}

describe("source health details", () => {
  it("groups failed sources with their website and localized reason", () => {
    const report = summary({
      failed_sources: ["榮總"],
      source_diagnostics: [{
        source: "榮總",
        status: "failed",
        unstable: false,
        failure_class: "parser_regression",
        failure_evidence: {
          url_host: "www.vghtpe.gov.tw",
          message: "來源解析錯誤：頁面未完成渲染",
        },
      }],
      route_attempts: [{
        source: "榮總",
        route_id: "official-browser",
        url: "https://www.vghtpe.gov.tw/News.action?gcode=A05",
        url_host: "www.vghtpe.gov.tw",
        status: "failed",
        failure_class: "parser_regression",
      }],
    });

    const [detail] = buildSourceHealthDetails(report, "failed");

    expect(detail.source).toBe("榮總");
    expect(detail.host).toBe("www.vghtpe.gov.tw");
    expect(detail.failureLabel).toBe("頁面解析失敗");
    expect(healthTooltip([detail])).toEqual(["榮總 · www.vghtpe.gov.tw"]);
  });

  it("shows an unstable primary failure and the successful fallback", () => {
    const report = summary({
      source_diagnostics: [{
        source: "環境部",
        status: "success",
        unstable: true,
        item_count: 8,
        last_failure_class: "parser_regression",
        failure_evidence: { url_host: "www.moenv.gov.tw", message: "主站未完成渲染" },
        final_route: {
          route_id: "news-portal",
          url: "https://enews.moenv.gov.tw/",
          url_host: "enews.moenv.gov.tw",
          used_fallback: true,
        },
      }],
      route_attempts: [
        {
          source: "環境部",
          route_id: "primary-browser",
          url: "https://www.moenv.gov.tw/press/press-releases/2626.html",
          url_host: "www.moenv.gov.tw",
          status: "failed",
          failure_class: "parser_regression",
        },
        {
          source: "環境部",
          route_id: "news-portal",
          url: "https://enews.moenv.gov.tw/",
          url_host: "enews.moenv.gov.tw",
          status: "success",
          item_count: 8,
        },
      ],
    });

    const [detail] = buildSourceHealthDetails(report, "unstable");

    expect(detail.host).toBe("www.moenv.gov.tw");
    expect(detail.usedFallback).toBe(true);
    expect(detail.finalHost).toBe("enews.moenv.gov.tw");
    expect(detail.itemCount).toBe(8);
  });

  it("counts repeated route attempts as retries", () => {
    const report = summary({
      source_diagnostics: [{ source: "經濟部", status: "success", unstable: true }],
      route_attempts: [
        { source: "經濟部", route_id: "official-browser", status: "failed", attempt_number: 1 },
        { source: "經濟部", route_id: "official-browser", status: "success", attempt_number: 2 },
      ],
    });

    expect(buildSourceHealthDetails(report, "unstable")[0].retryCount).toBe(1);
  });

  it("falls back to failed_sources for legacy summaries", () => {
    const [detail] = buildSourceHealthDetails(
      summary({ failed_sources: ["榮總"], source_diagnostics: undefined }),
      "failed",
    );

    expect(detail.source).toBe("榮總");
    expect(detail.host).toBe("");
  });

  it("toggles only cards that have details", () => {
    expect(toggleHealthPanel(null, "failed", 1)).toBe("failed");
    expect(toggleHealthPanel("failed", "failed", 1)).toBeNull();
    expect(toggleHealthPanel("failed", "unstable", 1)).toBe("unstable");
    expect(toggleHealthPanel(null, "failed", 0)).toBeNull();
    expect(failureClassLabel("browser_runtime")).toBe("瀏覽器執行失敗");
  });
});
