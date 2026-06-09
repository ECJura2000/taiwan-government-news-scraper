import json
import os
import threading
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any
from urllib.request import Request, urlopen

from requests.exceptions import ConnectionError, HTTPError, SSLError, Timeout

from .policy import get_zero_item_alert_runs

CURRENT_RUN_CONTEXT = ContextVar("news_scraper_run_context", default=None)


@dataclass
class RunContext:
    source_attempts: list[dict] = field(default_factory=list)
    insecure_ssl_hosts: set[str] = field(default_factory=set)
    failed_sources: list[str] = field(default_factory=list)
    quality_summary: dict = field(default_factory=dict)
    anomalies: list[dict] = field(default_factory=list)
    alerts: list[dict] = field(default_factory=list)
    retry_timeout_extra_seconds: int = 0
    lock: threading.RLock = field(default_factory=threading.RLock, repr=False)

    def record_source_attempt(self, source, attempt, item_count, elapsed_seconds, error=None):
        result = {
            "source": source,
            "attempt": attempt,
            "status": "failed" if error else "success",
            "item_count": item_count,
            "elapsed_seconds": round(elapsed_seconds, 3),
            "error_category": classify_error(error),
            "error_type": type(error).__name__ if error else "",
            "error_message": str(error) if error else "",
        }
        with self.lock:
            self.source_attempts.append(result)
        return result

    def record_insecure_ssl_use(self, host):
        if host:
            with self.lock:
                self.insecure_ssl_hosts.add(host)

    def snapshot_attempts(self):
        with self.lock:
            return [dict(result) for result in self.source_attempts]


@contextmanager
def use_run_context(context):
    token = CURRENT_RUN_CONTEXT.set(context)
    try:
        yield context
    finally:
        CURRENT_RUN_CONTEXT.reset(token)


def get_current_run_context():
    return CURRENT_RUN_CONTEXT.get()


def classify_error(error):
    if error is None:
        return ""
    if isinstance(error, SSLError):
        return "ssl"
    if isinstance(error, Timeout) or "timeout" in type(error).__name__.lower():
        return "timeout"
    if isinstance(error, HTTPError):
        return "http"
    if isinstance(error, ConnectionError):
        return "connection"

    error_name = type(error).__name__.lower()
    error_message = str(error).lower()
    if "webdriver" in error_name or "selenium" in error_name or "chromedriver" in error_message:
        return "browser"
    if isinstance(error, (ValueError, KeyError, TypeError)):
        return "parse"
    return "unexpected"


def build_run_report(*, context, started_at, finished_at, selected_sources, news_count, output_path):
    attempts = context.snapshot_attempts()
    error_counts: dict[str, int] = {}
    for attempt in attempts:
        category = attempt["error_category"]
        if category:
            error_counts[category] = error_counts.get(category, 0) + 1

    quality_requires_attention = bool(context.quality_summary.get("alert_reasons"))
    if context.failed_sources:
        status = "partial_failure"
    elif context.anomalies or quality_requires_attention:
        status = "attention"
    else:
        status = "success"

    return {
        "status": status,
        "started_at": started_at.isoformat(timespec="seconds"),
        "finished_at": finished_at.isoformat(timespec="seconds"),
        "duration_seconds": round((finished_at - started_at).total_seconds(), 3),
        "selected_source_count": len(selected_sources),
        "news_count": news_count,
        "failed_sources": list(context.failed_sources),
        "error_counts": error_counts,
        "insecure_ssl_hosts": sorted(context.insecure_ssl_hosts),
        "quality": context.quality_summary,
        "anomalies": list(context.anomalies),
        "alerts": list(context.alerts),
        "output_file": str(output_path),
        "source_attempts": attempts,
    }


def load_recent_reports(report_dir, limit=12):
    report_dir = Path(report_dir)
    reports = []
    for report_path in sorted(report_dir.glob("news_scraper_run_*.json"), reverse=True):
        try:
            reports.append(json.loads(report_path.read_text(encoding="utf-8")))
        except (OSError, ValueError):
            continue
        if len(reports) >= limit:
            break
    return reports


def detect_run_anomalies(context, selected_sources, recent_reports):
    source_counts = context.quality_summary.get("source_counts", {})
    failed_sources = set(context.failed_sources)
    anomalies = []

    for source in selected_sources:
        if source in failed_sources or source_counts.get(source, 0) != 0:
            continue
        required_zero_runs = get_zero_item_alert_runs(source)
        if required_zero_runs <= 0:
            continue

        consecutive_zero_runs = 1
        for report in recent_reports:
            previous_source_counts = report.get("quality", {}).get("source_counts", {})
            if source in previous_source_counts:
                if previous_source_counts[source] == 0:
                    consecutive_zero_runs += 1
                    if consecutive_zero_runs >= required_zero_runs:
                        break
                    continue
                break
            attempts = report.get("source_attempts", [])
            successful_attempts = [
                attempt
                for attempt in attempts
                if attempt.get("source") == source and attempt.get("status") == "success"
            ]
            if successful_attempts:
                if successful_attempts[-1].get("item_count") == 0:
                    consecutive_zero_runs += 1
                break
        if consecutive_zero_runs >= required_zero_runs:
            anomalies.append(
                {
                    "category": "consecutive_zero_items",
                    "source": source,
                    "zero_run_count": consecutive_zero_runs,
                    "threshold": required_zero_runs,
                    "message": "{} 連續 {} 次成功執行但抓到 0 筆，可能是網站改版。".format(
                        source,
                        consecutive_zero_runs,
                    ),
                }
            )

    durations = [
        report.get("duration_seconds")
        for report in recent_reports
        if isinstance(report.get("duration_seconds"), (int, float))
    ]
    if durations:
        average_duration = sum(durations) / len(durations)
        current_duration = sum(attempt["elapsed_seconds"] for attempt in context.snapshot_attempts())
        if current_duration > max(30, average_duration * 3):
            anomalies.append(
                {
                    "category": "slow_run",
                    "message": "本次來源總耗時 {:.1f} 秒，超過近期平均 {:.1f} 秒的三倍。".format(
                        current_duration,
                        average_duration,
                    ),
                }
            )

    context.anomalies = anomalies
    return anomalies


def build_alert_payload(report):
    return {
        "title": "每週新聞整理程式異常",
        "status": report["status"],
        "failed_sources": report["failed_sources"],
        "anomalies": report["anomalies"],
        "error_counts": report["error_counts"],
        "quality": report["quality"],
    }


def send_webhook_alert(payload, webhook_url=None, timeout=10):
    webhook_url = webhook_url or os.environ.get("NEWS_SCRAPER_ALERT_WEBHOOK", "")
    if not webhook_url:
        return {"status": "not_configured"}

    request = Request(
        webhook_url,
        data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urlopen(request, timeout=timeout) as response:
        return {"status": "sent", "http_status": response.status}


def should_send_alert(report):
    quality = report.get("quality", {})
    return bool(
        report.get("failed_sources")
        or report.get("anomalies")
        or quality.get("alert_reasons")
    )


def prune_old_reports(report_dir, retention_days=180, now=None):
    report_dir = Path(report_dir)
    cutoff = (now or datetime.now()) - timedelta(days=max(1, retention_days))
    removed = []
    for report_path in report_dir.glob("news_scraper_run_*.json"):
        modified_at = datetime.fromtimestamp(report_path.stat().st_mtime)
        if modified_at < cutoff:
            report_path.unlink()
            removed.append(str(report_path))
    return removed


def build_ssl_allowlist_audit(reports, allowed_hosts, minimum_reports=8):
    used_hosts = {
        host
        for report in reports
        for host in report.get("insecure_ssl_hosts", [])
    }
    enough_history = len(reports) >= minimum_reports
    return {
        "status": "ready" if enough_history else "insufficient_history",
        "minimum_reports": minimum_reports,
        "report_count": len(reports),
        "allowed_host_count": len(allowed_hosts),
        "used_in_recent_reports": sorted(used_hosts),
        "removal_candidates": sorted(set(allowed_hosts) - used_hosts) if enough_history else [],
    }


def build_trend_summary(reports, allowed_ssl_hosts=None):
    source_stats: dict[str, dict[str, Any]] = {}
    for report in reports:
        quality_source_counts = report.get("quality", {}).get("source_counts", {})
        for attempt in report.get("source_attempts", []):
            source = attempt.get("source")
            if not source:
                continue
            stats = source_stats.setdefault(
                source,
                {"attempts": 0, "successes": 0, "failures": 0, "zero_item_successes": 0, "elapsed_seconds": 0.0},
            )
            stats["attempts"] += 1
            stats["elapsed_seconds"] += float(attempt.get("elapsed_seconds") or 0)
            if attempt.get("status") == "success":
                stats["successes"] += 1
                final_item_count = quality_source_counts.get(source, attempt.get("item_count"))
                if final_item_count == 0:
                    stats["zero_item_successes"] += 1
            else:
                stats["failures"] += 1

    for stats in source_stats.values():
        stats["average_elapsed_seconds"] = round(stats.pop("elapsed_seconds") / stats["attempts"], 3)
        stats["success_rate"] = round(stats["successes"] / stats["attempts"], 4)
    summary = {"report_count": len(reports), "sources": source_stats}
    if allowed_ssl_hosts is not None:
        summary["ssl_allowlist_audit"] = build_ssl_allowlist_audit(reports, allowed_ssl_hosts)
    return summary


def write_json_file(data, path):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    return path


def write_run_report(report, report_dir):
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    return write_json_file(report, Path(report_dir) / "news_scraper_run_{}.json".format(timestamp))
