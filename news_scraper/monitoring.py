import json
import logging
import os
import re
import statistics
import threading
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import asdict, dataclass, field
from datetime import date, datetime, timedelta
from enum import Enum
from pathlib import Path
from typing import Any, TypedDict, cast
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from requests.exceptions import ConnectionError, HTTPError, SSLError, Timeout

from .config import AI_POLICY_RULESET_VERSION, get_ai_policy_ruleset_hash
from .io_utils import atomic_write_text
from .policy import get_summary_coverage_policy, get_zero_item_alert_runs

CURRENT_RUN_CONTEXT = ContextVar("news_scraper_run_context", default=None)
logger = logging.getLogger(__name__)
REPORT_SCHEMA_VERSION = 2


class QualitySummary(TypedDict, total=False):
    input_count: int
    output_count: int
    duplicate_count: int
    invalid_count: int
    excluded_non_news_count: int
    source_counts: dict[str, int]
    summary_count: int
    summary_coverage_rate: float
    date_source_counts: dict[str, int]
    description_fallback_count: int
    issues: list[dict[str, Any]]
    alert_reasons: list[str]


class AttemptStatus(str, Enum):
    SUCCESS = "success"
    FAILED = "failed"


class RunStatus(str, Enum):
    SUCCESS = "success"
    ATTENTION = "attention"
    PARTIAL_FAILURE = "partial_failure"
    CANCELLED = "cancelled"


class ErrorCategory(str, Enum):
    SSL = "ssl"
    TIMEOUT = "timeout"
    HTTP = "http"
    CONNECTION = "connection"
    BROWSER = "browser"
    PARSE = "parse"
    UNEXPECTED = "unexpected"


def new_quality_summary() -> QualitySummary:
    return cast(QualitySummary, {})


@dataclass(frozen=True)
class SourceAttempt:
    source: str
    attempt: int
    status: str
    item_count: int
    elapsed_seconds: float
    error_category: str = ""
    error_type: str = ""
    error_message: str = ""

    def to_dict(self):
        return asdict(self)


@dataclass(frozen=True)
class ParserWarning:
    category: str
    parser: str
    source: str
    value: str
    error_type: str = ""
    error_message: str = ""

    def to_dict(self):
        return asdict(self)

    def __getitem__(self, key):
        try:
            return getattr(self, key)
        except AttributeError as exc:
            raise KeyError(key) from exc


@dataclass
class RunContext:
    source_attempts: list[SourceAttempt] = field(default_factory=list)
    insecure_ssl_hosts: set[str] = field(default_factory=set)
    failed_sources: list[str] = field(default_factory=list)
    quality_summary: QualitySummary = field(default_factory=new_quality_summary)
    anomalies: list[dict] = field(default_factory=list)
    alerts: list[dict] = field(default_factory=list)
    parser_warnings: list[ParserWarning] = field(default_factory=list)
    scheduling_plan: list[dict] = field(default_factory=list)
    retry_timeout_extra_seconds: int = 0
    cancelled: bool = False
    lock: threading.RLock = field(default_factory=threading.RLock, repr=False)

    def record_source_attempt(self, source, attempt, item_count, elapsed_seconds, error=None):
        result = SourceAttempt(
            source=source,
            attempt=attempt,
            status=AttemptStatus.FAILED if error else AttemptStatus.SUCCESS,
            item_count=item_count,
            elapsed_seconds=round(elapsed_seconds, 3),
            error_category=classify_error(error),
            error_type=type(error).__name__ if error else "",
            error_message=str(error) if error else "",
        )
        with self.lock:
            self.source_attempts.append(result)
        return result

    def record_insecure_ssl_use(self, host):
        if host:
            with self.lock:
                self.insecure_ssl_hosts.add(host)

    def snapshot_attempts(self):
        with self.lock:
            return [result.to_dict() for result in self.source_attempts]

    def record_parser_warning(self, parser, value, error=None, source=""):
        warning = ParserWarning(
            category="parser_warning",
            parser=parser,
            source=source,
            value=str(value)[:500],
            error_type=type(error).__name__ if error else "",
            error_message=str(error) if error else "",
        )
        with self.lock:
            self.parser_warnings.append(warning)
        return warning


@contextmanager
def use_run_context(context):
    token = CURRENT_RUN_CONTEXT.set(context)
    try:
        yield context
    finally:
        CURRENT_RUN_CONTEXT.reset(token)


def get_current_run_context():
    return CURRENT_RUN_CONTEXT.get()


def record_parser_warning(parser, value, error=None, source=""):
    context = get_current_run_context()
    if context is not None:
        return context.record_parser_warning(parser, value, error=error, source=source)
    logger.warning("%s 解析失敗：%r；原因：%s", parser, value, error or "unknown")
    return None


def classify_error(error):
    if error is None:
        return ""
    structured_category = getattr(error, "error_category", "")
    if structured_category:
        try:
            return ErrorCategory(structured_category)
        except ValueError:
            return ErrorCategory.UNEXPECTED
    if isinstance(error, SSLError):
        return ErrorCategory.SSL
    if isinstance(error, Timeout) or "timeout" in type(error).__name__.lower():
        return ErrorCategory.TIMEOUT
    if isinstance(error, HTTPError):
        return ErrorCategory.HTTP
    if isinstance(error, ConnectionError):
        return ErrorCategory.CONNECTION

    error_name = type(error).__name__.lower()
    error_message = str(error).lower()
    if "webdriver" in error_name or "selenium" in error_name or "chromedriver" in error_message:
        return ErrorCategory.BROWSER
    if isinstance(error, (ValueError, KeyError, TypeError)):
        return ErrorCategory.PARSE
    return ErrorCategory.UNEXPECTED


def build_run_report(
    *,
    context,
    started_at,
    finished_at,
    selected_sources,
    news_count,
    output_path,
    week_start: date | None = None,
    week_end: date | None = None,
):
    attempts = context.snapshot_attempts()
    error_counts: dict[str, int] = {}
    for attempt in attempts:
        category = attempt["error_category"]
        if category:
            error_counts[category] = error_counts.get(category, 0) + 1

    quality_requires_attention = bool(context.quality_summary.get("alert_reasons"))
    if context.cancelled:
        status = RunStatus.CANCELLED
    elif context.failed_sources:
        status = RunStatus.PARTIAL_FAILURE
    elif context.anomalies or context.parser_warnings or quality_requires_attention:
        status = RunStatus.ATTENTION
    else:
        status = RunStatus.SUCCESS

    return {
        "report_schema_version": REPORT_SCHEMA_VERSION,
        "status": status,
        "started_at": started_at.isoformat(timespec="seconds"),
        "finished_at": finished_at.isoformat(timespec="seconds"),
        "duration_seconds": round((finished_at - started_at).total_seconds(), 3),
        "selected_source_count": len(selected_sources),
        "selected_sources": list(selected_sources),
        "week_start": week_start.isoformat() if week_start else "",
        "week_end": week_end.isoformat() if week_end else "",
        "news_count": news_count,
        "failed_sources": list(context.failed_sources),
        "error_counts": error_counts,
        "insecure_ssl_hosts": sorted(context.insecure_ssl_hosts),
        "quality": context.quality_summary,
        "ai_policy": {
            "version": AI_POLICY_RULESET_VERSION,
            "ruleset_hash": get_ai_policy_ruleset_hash(),
        },
        "anomalies": list(context.anomalies),
        "parser_warnings": [warning.to_dict() for warning in context.parser_warnings],
        "scheduling_plan": list(context.scheduling_plan),
        "alerts": list(context.alerts),
        "output_file": str(output_path) if output_path else "",
        "source_attempts": attempts,
    }


def load_recent_reports(report_dir, limit=12):
    report_dir = Path(report_dir)
    reports = []
    for report_path in sorted(report_dir.glob("news_scraper_run_*.json"), reverse=True):
        try:
            report = json.loads(report_path.read_text(encoding="utf-8"))
            validate_run_report(report)
            reports.append(report)
        except (OSError, ValueError):
            continue
        if len(reports) >= limit:
            break
    return reports


def validate_run_report(report):
    if not isinstance(report, dict):
        raise ValueError("run report 必須是 JSON object")
    required = {"status", "source_attempts", "quality"}
    missing = required - set(report)
    if missing:
        raise ValueError(f"run report 缺少欄位：{sorted(missing)}")
    RunStatus(report["status"])
    if not isinstance(report["source_attempts"], list) or not isinstance(report["quality"], dict):
        raise ValueError("run report 的 source_attempts/quality 型別錯誤")


def _report_window(report: dict[str, Any], fallback_key: str) -> tuple[str, str, str]:
    week_start = str(report.get("week_start") or "")
    week_end = str(report.get("week_end") or "")
    if week_start and week_end:
        return "{}:{}".format(week_start, week_end), week_start, week_end

    output_file = str(report.get("output_file") or "")
    match = re.search(r"（(\d{3})(\d{2})(\d{2})至(\d{3})(\d{2})(\d{2})）", output_file)
    if match:
        start_year, start_month, start_day, end_year, end_month, end_day = (int(value) for value in match.groups())
        inferred_start = date(start_year + 1911, start_month, start_day).isoformat()
        inferred_end = date(end_year + 1911, end_month, end_day).isoformat()
        return "{}:{}".format(inferred_start, inferred_end), inferred_start, inferred_end
    return fallback_key, "", ""


def _source_finished_successfully(report: dict[str, Any], source: str) -> bool:
    attempts = [attempt for attempt in report.get("source_attempts", []) if attempt.get("source") == source]
    if attempts:
        return attempts[-1].get("status") == AttemptStatus.SUCCESS
    if source in report.get("failed_sources", []):
        return False
    return source in report.get("quality", {}).get("source_counts", {})


def _source_output_count(report: dict[str, Any], source: str) -> int | None:
    source_counts = report.get("quality", {}).get("source_counts", {})
    if source in source_counts:
        value = source_counts[source]
        return int(value) if isinstance(value, (int, float)) else None
    attempts = [attempt for attempt in report.get("source_attempts", []) if attempt.get("source") == source]
    if attempts and attempts[-1].get("status") == AttemptStatus.SUCCESS:
        value = attempts[-1].get("item_count")
        return int(value) if isinstance(value, (int, float)) else None
    return None


def _append_summary_coverage_anomaly(
    anomalies: list[dict[str, Any]],
    context: RunContext,
    selected_sources: list[str],
    recent_reports: list[dict[str, Any]],
) -> None:
    policy = get_summary_coverage_policy()
    minimum_history = int(policy.get("minimum_history", 3))
    minimum_output_count = int(policy.get("minimum_output_count", 20))
    drop_ratio = float(policy.get("drop_ratio", 0.20))
    current_output_count = int(context.quality_summary.get("output_count", 0))
    current_coverage = float(context.quality_summary.get("summary_coverage_rate", 0.0))
    if current_output_count < minimum_output_count:
        return

    selected_source_set = set(selected_sources)

    def has_matching_sources(report: dict[str, Any]) -> bool:
        historical_sources = report.get("selected_sources")
        if isinstance(historical_sources, list):
            return set(historical_sources) == selected_source_set
        return report.get("selected_source_count") == len(selected_sources)

    historical_coverages = [
        float(report.get("quality", {}).get("summary_coverage_rate"))
        for report in recent_reports
        if has_matching_sources(report)
        and not report.get("failed_sources")
        and isinstance(report.get("quality", {}).get("summary_coverage_rate"), (int, float))
    ]
    if len(historical_coverages) < minimum_history:
        return
    reference_coverage = statistics.median(historical_coverages[:12])
    if current_coverage < reference_coverage - drop_ratio:
        anomalies.append(
            {
                "category": "summary_coverage_drop",
                "current_coverage_rate": round(current_coverage, 4),
                "reference_coverage_rate": round(reference_coverage, 4),
                "drop_threshold": drop_ratio,
                "history_count": min(len(historical_coverages), 12),
                "message": "摘要覆蓋率 {:.1%}，低於近期中位數 {:.1%}。".format(
                    current_coverage,
                    reference_coverage,
                ),
            }
        )


def detect_run_anomalies(
    context,
    selected_sources,
    recent_reports,
    week_start: date | None = None,
    week_end: date | None = None,
):
    source_counts = context.quality_summary.get("source_counts", {})
    failed_sources = set(context.failed_sources)
    anomalies: list[dict[str, Any]] = []

    for source in selected_sources:
        if source in failed_sources or source_counts.get(source, 0) != 0:
            continue
        required_zero_runs = get_zero_item_alert_runs(source)
        if required_zero_runs <= 0:
            continue

        current_window = (
            "{}:{}".format(week_start.isoformat(), week_end.isoformat())
            if week_start and week_end
            else "current"
        )
        evidence_windows = [
            {
                "week_start": week_start.isoformat() if week_start else "",
                "week_end": week_end.isoformat() if week_end else "",
            }
        ]
        seen_windows = {current_window}
        for index, report in enumerate(recent_reports):
            window_key, previous_week_start, previous_week_end = _report_window(report, "legacy:{}".format(index))
            if window_key in seen_windows:
                continue
            seen_windows.add(window_key)
            if not _source_finished_successfully(report, source):
                break
            if _source_output_count(report, source) != 0:
                break
            evidence_windows.append(
                {
                    "week_start": previous_week_start,
                    "week_end": previous_week_end,
                }
            )
            if len(evidence_windows) >= required_zero_runs:
                break
        if len(evidence_windows) >= required_zero_runs:
            anomalies.append(
                {
                    "category": "consecutive_zero_items",
                    "source": source,
                    "zero_run_count": len(evidence_windows),
                    "distinct_window_count": len(evidence_windows),
                    "threshold": required_zero_runs,
                    "evidence_windows": evidence_windows,
                    "message": "{} 連續 {} 個不同週期成功執行但抓到 0 筆，可能是網站改版。".format(
                        source,
                        len(evidence_windows),
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

    _append_summary_coverage_anomaly(anomalies, context, selected_sources, recent_reports)
    context.anomalies = anomalies
    return anomalies


def build_alert_payload(report):
    return {
        "title": "每週新聞整理程式異常",
        "status": report["status"],
        "failed_sources": report["failed_sources"],
        "anomalies": report["anomalies"],
        "parser_warnings": report.get("parser_warnings", []),
        "error_counts": report["error_counts"],
        "quality": report["quality"],
    }


def send_webhook_alert(payload, webhook_url=None, timeout=10):
    webhook_url = webhook_url or os.environ.get("NEWS_SCRAPER_ALERT_WEBHOOK", "")
    if not webhook_url:
        return {"status": "not_configured"}

    parsed_url = urlparse(webhook_url)
    if parsed_url.scheme != "https" or not parsed_url.hostname:
        raise ValueError("異常 webhook 必須使用有效的 HTTPS URL。")

    request = Request(
        webhook_url,
        data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    # The URL was restricted to HTTPS above; urllib is retained to avoid another runtime dependency.
    with urlopen(request, timeout=timeout) as response:  # nosec B310
        return {"status": "sent", "http_status": response.status}


def should_send_alert(report):
    quality = report.get("quality", {})
    return bool(
        report.get("failed_sources")
        or report.get("anomalies")
        or report.get("parser_warnings")
        or quality.get("alert_reasons")
    )


def prune_old_reports(report_dir, retention_days=180, now=None):
    report_dir = Path(report_dir)
    cutoff = (now or datetime.now()) - timedelta(days=max(1, retention_days))
    removed = []
    for report_path in report_dir.glob("news_scraper_run_*.json"):
        if report_path.is_symlink() or not report_path.is_file():
            continue
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
    return atomic_write_text(path, json.dumps(data, ensure_ascii=False, indent=2))


def write_run_report(report, report_dir):
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    return write_json_file(report, Path(report_dir) / "news_scraper_run_{}.json".format(timestamp))
