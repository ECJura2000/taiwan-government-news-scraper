import threading
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Callable

from .paths import prepare_workspace


@dataclass(frozen=True)
class RunOptions:
    sources: tuple[str, ...] | None = None
    output_dir: Path | None = None
    report_dir: Path | None = None
    max_workers: int | None = None
    dedupe_affiliated: bool = False
    report_retention_days: int = 180
    fail_on_source_error: bool = False
    alert_webhook: str | None = None
    mode: str = "headless"


@dataclass(frozen=True)
class ProgressEvent:
    kind: str
    message: str
    source: str = ""
    completed: int = 0
    total: int = 0
    attempt: int = 1


@dataclass(frozen=True)
class RunResult:
    status: str
    output_path: Path | None
    report_path: Path | None
    news_count: int
    failed_sources: tuple[str, ...] = ()
    anomalies: tuple[dict, ...] = ()
    quality: dict = field(default_factory=dict)
    insecure_ssl_hosts: tuple[str, ...] = ()
    cancelled: bool = False
    news_items: tuple[dict, ...] = field(default_factory=tuple, repr=False)

    def to_summary(self) -> dict:
        quality_summary = dict(self.quality)
        if "issues" in quality_summary:
            quality_summary["issue_count"] = len(quality_summary.pop("issues") or [])
        return {
            "status": self.status,
            "output_file": str(self.output_path) if self.output_path else "",
            "report_file": str(self.report_path) if self.report_path else "",
            "news_count": self.news_count,
            "failed_sources": list(self.failed_sources),
            "anomalies": list(self.anomalies),
            "quality": quality_summary,
            "insecure_ssl_hosts": list(self.insecure_ssl_hosts),
            "cancelled": self.cancelled,
        }


ProgressCallback = Callable[[ProgressEvent], None]


def _emit(callback: ProgressCallback | None, event: ProgressEvent) -> None:
    if callback is not None:
        callback(event)


def run_news_scraper(
    options: RunOptions,
    *,
    cancel_event: threading.Event | None = None,
    progress_callback: ProgressCallback | None = None,
) -> RunResult:
    from .config import SSL_FALLBACK_HOSTS
    from .excel_exporter import export_to_excel
    from .main import collect_all_this_week_news, normalize_selected_sources
    from .monitoring import (
        RunContext,
        build_alert_payload,
        build_run_report,
        build_trend_summary,
        detect_run_anomalies,
        load_recent_reports,
        prune_old_reports,
        send_webhook_alert,
        should_send_alert,
        write_json_file,
        write_run_report,
    )
    from .quality import process_news_quality
    from .run_lock import RunLock
    from .runtime import validate_runtime_environment

    cancel_event = cancel_event or threading.Event()
    selected_sources = normalize_selected_sources(options.sources)
    validate_runtime_environment(selected_sources=selected_sources, needs_excel_export=True)

    workspace = prepare_workspace()
    output_dir = Path(options.output_dir) if options.output_dir else workspace.output
    report_dir = Path(options.report_dir) if options.report_dir else output_dir / "執行紀錄"
    output_dir.mkdir(parents=True, exist_ok=True)
    report_dir.mkdir(parents=True, exist_ok=True)

    _emit(progress_callback, ProgressEvent("start", "開始整理新聞", total=len(selected_sources)))
    if workspace.used_fallback:
        _emit(
            progress_callback,
            ProgressEvent("warning", "原程式位置不可寫，已改用 {}".format(workspace.root)),
        )

    started_at = datetime.now().astimezone()
    context = RunContext()
    report_path: Path | None = None
    output_path: Path | None = None

    def on_source_progress(source: str, completed: int, total: int, attempt: int) -> None:
        _emit(
            progress_callback,
            ProgressEvent(
                "source",
                "{}完成：{}".format("重試" if attempt > 1 else "", source),
                source=source,
                completed=completed,
                total=total,
                attempt=attempt,
            ),
        )

    with (
        RunLock(workspace.program_data / "run.lock", mode=options.mode),
        RunLock(output_dir / ".news-scraper.run.lock", mode=options.mode),
    ):
        recent_reports = load_recent_reports(report_dir)
        news = collect_all_this_week_news(
            selected_sources=selected_sources,
            max_workers=options.max_workers,
            dedupe_affiliated=options.dedupe_affiliated,
            context=context,
            recent_reports=recent_reports,
            cancel_event=cancel_event,
            progress_callback=on_source_progress,
        )
        news, context.quality_summary = process_news_quality(news, selected_sources)

        if cancel_event.is_set():
            context.cancelled = True
            _emit(progress_callback, ProgressEvent("cancelled", "已取消；不產生正式 Excel。"))
        else:
            _emit(progress_callback, ProgressEvent("export", "正在產生 Excel。"))
            output_path = export_to_excel(
                news,
                output_dir=output_dir,
                dedupe_affiliated=options.dedupe_affiliated,
            )
            detect_run_anomalies(context, selected_sources, recent_reports)

        finished_at = datetime.now().astimezone()
        report = build_run_report(
            context=context,
            started_at=started_at,
            finished_at=finished_at,
            selected_sources=selected_sources,
            news_count=len(news),
            output_path=output_path,
        )
        if not context.cancelled and should_send_alert(report):
            try:
                alert_result = send_webhook_alert(
                    build_alert_payload(report),
                    webhook_url=options.alert_webhook,
                )
            except Exception as exc:
                alert_result = {"status": "failed", "error_type": type(exc).__name__}
            context.alerts.append(alert_result)
            report["alerts"] = list(context.alerts)

        report_path = write_run_report(report, report_dir)
        prune_old_reports(report_dir, retention_days=options.report_retention_days)
        trend_reports = recent_reports if context.cancelled else [report] + recent_reports
        write_json_file(
            build_trend_summary(trend_reports[:52], allowed_ssl_hosts=SSL_FALLBACK_HOSTS),
            report_dir / "trend_summary.json",
        )

    raw_status = report["status"]
    status = raw_status.value if hasattr(raw_status, "value") else str(raw_status)
    result = RunResult(
        status=status,
        output_path=output_path,
        report_path=report_path,
        news_count=len(news),
        failed_sources=tuple(context.failed_sources),
        anomalies=tuple(context.anomalies),
        quality=dict(context.quality_summary),
        insecure_ssl_hosts=tuple(sorted(context.insecure_ssl_hosts)),
        cancelled=context.cancelled,
        news_items=tuple(news),
    )
    _emit(progress_callback, ProgressEvent("done", "新聞整理完成。"))
    return result
