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

from .errors import ParserContractError
from .io_utils import atomic_write_text
from .policy import get_summary_coverage_policy, get_zero_item_alert_runs

CURRENT_RUN_CONTEXT = ContextVar("news_scraper_run_context", default=None)
logger = logging.getLogger(__name__)
REPORT_SCHEMA_VERSION = 4
TREND_SCHEMA_VERSION = 2


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


class FailureClass(str, Enum):
    SOURCE_OUTAGE = "source_outage"
    RUNNER_NETWORK = "runner_network"
    TLS_CERTIFICATE = "tls_certificate"
    ACCESS_BLOCKED = "access_blocked"
    PARSER_REGRESSION = "parser_regression"
    BROWSER_RUNTIME = "browser_runtime"
    UNKNOWN = "unknown"


def _failure_class_value(value) -> str:
    return value.value if isinstance(value, FailureClass) else str(value or "")


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
    route_id: str = ""
    url_host: str = ""
    http_status: int | None = None
    failure_class: str = ""
    failure_evidence: dict[str, Any] = field(default_factory=dict)

    def to_dict(self):
        return asdict(self)


@dataclass(frozen=True)
class RouteAttempt:
    source: str
    route_id: str
    url: str
    url_host: str
    kind: str
    parser: str
    priority: int
    official: bool
    coverage_reduced: bool
    status: str
    item_count: int
    elapsed_seconds: float
    error_category: str = ""
    failure_class: str = ""
    http_status: int | None = None
    content_type: str = ""
    response_bytes: int = 0
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
    route_attempts: list[RouteAttempt] = field(default_factory=list)
    final_routes: dict[str, dict[str, Any]] = field(default_factory=dict)
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
        evidence = extract_failure_evidence(error)
        if error is None:
            final_route = self.final_routes.get(source, {})
            evidence["route_id"] = str(final_route.get("route_id") or "")
            evidence["url_host"] = str(final_route.get("url_host") or "")
        result = SourceAttempt(
            source=source,
            attempt=attempt,
            status=AttemptStatus.FAILED if error else AttemptStatus.SUCCESS,
            item_count=item_count,
            elapsed_seconds=round(elapsed_seconds, 3),
            error_category=classify_error(error),
            error_type=type(error).__name__ if error else "",
            error_message=str(error) if error else "",
            route_id=str(evidence.get("route_id") or ""),
            url_host=str(evidence.get("url_host") or ""),
            http_status=evidence.get("http_status"),
            failure_class=classify_failure(error),
            failure_evidence=evidence,
        )
        with self.lock:
            self.source_attempts.append(result)
        return result

    def record_route_attempt(self, source, route, elapsed_seconds, item_count=0, error=None):
        evidence = extract_failure_evidence(error)
        result = RouteAttempt(
            source=source,
            route_id=route.id,
            url=route.url,
            url_host=urlparse(route.url).netloc.lower(),
            kind=route.kind,
            parser=route.parser,
            priority=route.priority,
            official=route.official,
            coverage_reduced=route.coverage_reduced,
            status=AttemptStatus.FAILED if error else AttemptStatus.SUCCESS,
            item_count=item_count,
            elapsed_seconds=round(elapsed_seconds, 3),
            error_category=classify_error(error),
            failure_class=classify_failure(error),
            http_status=evidence.get("http_status"),
            content_type=str(evidence.get("content_type") or ""),
            response_bytes=int(evidence.get("response_bytes") or 0),
            error_type=type(error).__name__ if error else "",
            error_message=str(error) if error else "",
        )
        with self.lock:
            self.route_attempts.append(result)
        return result

    def record_final_route(self, source, route):
        with self.lock:
            self.final_routes[source] = {
                "route_id": route.id,
                "url_host": urlparse(route.url).netloc.lower(),
                "kind": route.kind,
                "parser": route.parser,
                "used_fallback": route.priority > 1,
                "coverage_reduced": route.coverage_reduced,
            }

    def record_insecure_ssl_use(self, host):
        if host:
            with self.lock:
                self.insecure_ssl_hosts.add(host)

    def snapshot_attempts(self):
        with self.lock:
            return [result.to_dict() for result in self.source_attempts]

    def snapshot_route_attempts(self):
        with self.lock:
            return [result.to_dict() for result in self.route_attempts]

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
    if type(error).__module__.startswith("selenium."):
        return ErrorCategory.BROWSER
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


def _http_status(error) -> int | None:
    status = getattr(error, "http_status", None)
    response = getattr(error, "response", None)
    if status is None and response is not None:
        status = getattr(response, "status_code", None)
    try:
        return int(status) if status is not None else None
    except (TypeError, ValueError):
        return None


def extract_failure_evidence(error) -> dict[str, Any]:
    if error is None:
        return {}
    response = getattr(error, "response", None)
    headers = getattr(response, "headers", {}) if response is not None else {}
    content = getattr(response, "content", b"") if response is not None else b""
    url = str(
        getattr(error, "url", "")
        or getattr(getattr(error, "request", None), "url", "")
        or getattr(getattr(response, "request", None), "url", "")
        or getattr(response, "url", "")
    )
    response_bytes = getattr(error, "response_bytes", None)
    if response_bytes is None:
        try:
            response_bytes = len(content)
        except TypeError:
            response_bytes = 0
    return {
        "route_id": str(getattr(error, "route_id", "") or ""),
        "url_host": str(getattr(error, "url_host", "") or urlparse(url).netloc.lower()),
        "http_status": _http_status(error),
        "content_type": str(getattr(error, "content_type", "") or headers.get("Content-Type", "")),
        "response_bytes": int(response_bytes or 0),
        "selector": str(getattr(error, "selector", "") or ""),
    }


def classify_failure(error):
    if error is None:
        return ""
    structured = getattr(error, "failure_class", "")
    if structured:
        try:
            return FailureClass(structured)
        except ValueError:
            return FailureClass.UNKNOWN
    if isinstance(error, ParserContractError):
        return FailureClass.PARSER_REGRESSION
    category = classify_error(error)
    status = _http_status(error)
    message = str(error).lower()
    if category == ErrorCategory.SSL:
        return FailureClass.TLS_CERTIFICATE
    if category == ErrorCategory.BROWSER:
        return FailureClass.BROWSER_RUNTIME
    if category == ErrorCategory.PARSE:
        return FailureClass.UNKNOWN
    if category == ErrorCategory.HTTP:
        if status in {401, 403, 407, 409, 423, 429, 451} or any(
            marker in message for marker in ("forbidden", "too many requests", "captcha", "access denied")
        ):
            return FailureClass.ACCESS_BLOCKED
        if status is not None and status >= 500:
            return FailureClass.SOURCE_OUTAGE
        return FailureClass.UNKNOWN
    if category in {ErrorCategory.TIMEOUT, ErrorCategory.CONNECTION}:
        return FailureClass.SOURCE_OUTAGE
    return FailureClass.UNKNOWN


def _build_source_diagnostics(context, selected_sources, attempts):
    route_attempts = context.snapshot_route_attempts()
    diagnostics = []
    for source in selected_sources:
        source_attempts = [item for item in attempts if item.get("source") == source]
        source_routes = [item for item in route_attempts if item.get("source") == source]
        final_attempt = source_attempts[-1] if source_attempts else {}
        failed_attempts = [item for item in source_attempts if item.get("status") == AttemptStatus.FAILED]
        failed_routes = [item for item in source_routes if item.get("status") == AttemptStatus.FAILED]
        latest_failure = failed_attempts[-1] if failed_attempts else (failed_routes[-1] if failed_routes else {})
        final_route = dict(context.final_routes.get(source, {}))
        source_failed = source in context.failed_sources
        failure_evidence = dict(latest_failure.get("failure_evidence") or {})
        for key in ("url_host", "http_status", "content_type", "response_bytes"):
            if key not in failure_evidence and latest_failure.get(key) not in (None, ""):
                failure_evidence[key] = latest_failure[key]
        diagnostics.append(
            {
                "source": source,
                "status": "failed" if source_failed else "success",
                "unstable": not source_failed and bool(failed_attempts),
                "item_count": int(context.quality_summary.get("source_counts", {}).get(source, 0)),
                "attempt_count": len(source_attempts),
                "failure_class": final_attempt.get("failure_class", "") if source_failed else "",
                "last_failure_class": latest_failure.get("failure_class", ""),
                "error_category": final_attempt.get("error_category", "") if source_failed else "",
                "failure_evidence": failure_evidence,
                "elapsed_seconds": round(sum(float(item.get("elapsed_seconds") or 0) for item in source_attempts), 3),
                "final_route": final_route,
                "route_attempt_count": len(source_routes),
                "route_failure_classes": sorted(
                    {
                        _failure_class_value(item.get("failure_class"))
                        for item in failed_routes
                        if item.get("failure_class")
                    }
                ),
            }
        )
    return diagnostics, route_attempts


def network_control_available(timeout=5) -> bool:
    try:
        request = Request("https://www.gov.tw/", headers={"User-Agent": "news-scraper-health-check"})
        with urlopen(request, timeout=timeout) as response:  # nosec B310
            return int(response.status) < 500
    except Exception:
        return False


def _apply_runner_network_classification(source_diagnostics):
    outage_hosts = {
        str(item.get("failure_evidence", {}).get("url_host") or "")
        for item in source_diagnostics
        if item.get("status") == "failed" and item.get("failure_class") == FailureClass.SOURCE_OUTAGE
    }
    outage_hosts.discard("")
    if len(outage_hosts) < 3 or network_control_available():
        return
    for item in source_diagnostics:
        if item.get("status") == "failed" and item.get("failure_class") == FailureClass.SOURCE_OUTAGE:
            item["failure_class"] = FailureClass.RUNNER_NETWORK
            item["last_failure_class"] = FailureClass.RUNNER_NETWORK
            item["failure_evidence"]["network_control_probe"] = "failed"


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
    relevance_policy: dict | None = None,
):
    attempts = context.snapshot_attempts()
    error_counts: dict[str, int] = {}
    for attempt in attempts:
        category = attempt["error_category"]
        if category:
            error_counts[category] = error_counts.get(category, 0) + 1
    source_diagnostics, route_attempts = _build_source_diagnostics(context, selected_sources, attempts)
    _apply_runner_network_classification(source_diagnostics)
    failure_pairs = {
        (str(item["source"]), _failure_class_value(item["failure_class"]))
        for item in attempts
        if item.get("status") == AttemptStatus.FAILED and item.get("failure_class")
    }
    for item in source_diagnostics:
        if item.get("failure_class") == FailureClass.RUNNER_NETWORK:
            failure_pairs.discard((str(item["source"]), FailureClass.SOURCE_OUTAGE.value))
    failure_pairs.update(
        (str(item["source"]), _failure_class_value(item["failure_class"]))
        for item in source_diagnostics
        if item.get("failure_class")
    )
    failure_pairs.update(
        (str(item["source"]), _failure_class_value(item["failure_class"]))
        for item in route_attempts
        if item.get("status") == AttemptStatus.FAILED and item.get("failure_class")
    )
    failure_class_counts: dict[str, int] = {}
    for _source, failure_class in failure_pairs:
        failure_class_counts[failure_class] = failure_class_counts.get(failure_class, 0) + 1
    source_health = {
        "healthy_count": sum(
            item["status"] == "success" and not item["unstable"] for item in source_diagnostics
        ),
        "unstable_count": sum(bool(item["unstable"]) for item in source_diagnostics),
        "failed_count": sum(item["status"] == "failed" for item in source_diagnostics),
        "fallback_source_count": sum(bool(item["final_route"].get("used_fallback")) for item in source_diagnostics),
        "coverage_reduced_count": sum(bool(item["final_route"].get("coverage_reduced")) for item in source_diagnostics),
        "ssl_fallback_host_count": len(context.insecure_ssl_hosts),
    }

    quality_requires_attention = bool(context.quality_summary.get("alert_reasons"))
    if context.cancelled:
        status = RunStatus.CANCELLED
    elif context.failed_sources:
        status = RunStatus.PARTIAL_FAILURE
    elif context.anomalies or context.parser_warnings or quality_requires_attention:
        status = RunStatus.ATTENTION
    else:
        status = RunStatus.SUCCESS

    if relevance_policy is None:
        from .relevance import (
            build_default_relevance_profile,
            get_relevance_profile_summary,
        )

        relevance_policy = get_relevance_profile_summary(
            build_default_relevance_profile(),
        )

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
        "failure_class_counts": failure_class_counts,
        "insecure_ssl_hosts": sorted(context.insecure_ssl_hosts),
        "quality": context.quality_summary,
        "relevance_policy": relevance_policy,
        "ai_policy": {
            "version": relevance_policy["template_version"],
            "ruleset_hash": relevance_policy["ruleset_hash"],
        },
        "anomalies": list(context.anomalies),
        "parser_warnings": [warning.to_dict() for warning in context.parser_warnings],
        "scheduling_plan": list(context.scheduling_plan),
        "alerts": list(context.alerts),
        "output_file": str(output_path) if output_path else "",
        "source_attempts": attempts,
        "source_diagnostics": source_diagnostics,
        "route_attempts": route_attempts,
        "source_health": source_health,
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
    current_duration_seconds: float | None = None,
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

    selected_source_set = set(selected_sources)
    durations = [
        float(report["duration_seconds"])
        for report in recent_reports
        if isinstance(report.get("duration_seconds"), (int, float))
        and not report.get("failed_sources")
        and report.get("status") != "cancelled"
        and set(report.get("selected_sources") or []) == selected_source_set
        and int(report.get("selected_source_count") or 0) == len(selected_source_set)
    ]
    if current_duration_seconds is not None and not failed_sources and len(durations) >= 3:
        reference_duration = statistics.median(durations)
        threshold = max(reference_duration * 3, reference_duration + 60)
        if current_duration_seconds > threshold:
            anomalies.append(
                {
                    "category": "slow_run",
                    "current_duration_seconds": round(current_duration_seconds, 3),
                    "reference_duration_seconds": round(reference_duration, 3),
                    "history_count": len(durations),
                    "threshold_seconds": round(threshold, 3),
                    "message": "本次實際執行 {:.1f} 秒，超過相同來源近期中位數 {:.1f} 秒的告警門檻。".format(
                        current_duration_seconds,
                        reference_duration,
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
        diagnostics = report.get("source_diagnostics")
        if not isinstance(diagnostics, list):
            diagnostics = []
            source_names = report.get("selected_sources") or [
                item.get("source") for item in report.get("source_attempts", [])
            ]
            for source in dict.fromkeys(source_names):
                attempts = [
                    item for item in report.get("source_attempts", []) if item.get("source") == source
                ]
                final = attempts[-1] if attempts else {}
                diagnostics.append(
                    {
                        "source": source,
                        "status": "failed" if source in report.get("failed_sources", []) else "success",
                        "item_count": report.get("quality", {}).get("source_counts", {}).get(
                            source, final.get("item_count", 0)
                        ),
                        "elapsed_seconds": sum(float(item.get("elapsed_seconds") or 0) for item in attempts),
                        "failure_class": final.get("failure_class", ""),
                        "final_route": {},
                    }
                )
        for diagnostic in diagnostics:
            source = diagnostic.get("source")
            if not source:
                continue
            stats = source_stats.setdefault(
                source,
                {
                    "runs": 0,
                    "successes": 0,
                    "failures": 0,
                    "zero_item_successes": 0,
                    "elapsed_seconds": 0.0,
                    "fallback_uses": 0,
                    "coverage_reduced_uses": 0,
                    "last_failure_class": "",
                    "last_status": "",
                    "last_route": "",
                },
            )
            stats["runs"] += 1
            stats["elapsed_seconds"] += float(diagnostic.get("elapsed_seconds") or 0)
            stats["last_status"] = stats["last_status"] or str(diagnostic.get("status") or "")
            final_route = diagnostic.get("final_route") or {}
            stats["last_route"] = stats["last_route"] or str(final_route.get("route_id") or "")
            if final_route.get("used_fallback"):
                stats["fallback_uses"] += 1
            if final_route.get("coverage_reduced"):
                stats["coverage_reduced_uses"] += 1
            if diagnostic.get("status") == "success":
                stats["successes"] += 1
                if int(diagnostic.get("item_count") or 0) == 0:
                    stats["zero_item_successes"] += 1
            else:
                stats["failures"] += 1
                stats["last_failure_class"] = stats["last_failure_class"] or str(
                    diagnostic.get("failure_class") or ""
                )

    for stats in source_stats.values():
        stats["average_elapsed_seconds"] = round(stats.pop("elapsed_seconds") / stats["runs"], 3)
        stats["success_rate"] = round(stats["successes"] / stats["runs"], 4)
    summary = {
        "trend_schema_version": TREND_SCHEMA_VERSION,
        "report_count": len(reports),
        "sources": source_stats,
    }
    if allowed_ssl_hosts is not None:
        summary["ssl_allowlist_audit"] = build_ssl_allowlist_audit(reports, allowed_ssl_hosts)
    return summary


def write_json_file(data, path):
    path = Path(path)
    return atomic_write_text(path, json.dumps(data, ensure_ascii=False, indent=2))


def write_run_report(report, report_dir):
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    return write_json_file(report, Path(report_dir) / "news_scraper_run_{}.json".format(timestamp))
