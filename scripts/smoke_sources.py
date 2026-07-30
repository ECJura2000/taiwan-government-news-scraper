from __future__ import annotations

import argparse
from datetime import date, datetime
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from news_scraper.scrapers.registry import SCRAPER_REGISTRY

HIGH_RISK_SOURCES = ("榮總", "司法院", "財政部")
DEFAULT_BATCH_COUNT = 7
BLOCKING_FAILURE_CLASSES = {"parser_regression", "browser_runtime", "unknown"}
WARNING_FAILURE_CLASSES = {
    "source_outage",
    "runner_network",
    "tls_certificate",
    "access_blocked",
}


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run rotating live parser smoke checks.")
    parser.add_argument("--batch-count", type=int, default=DEFAULT_BATCH_COUNT)
    parser.add_argument("--batch-index", type=int)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--retry-delay", type=float, default=10)
    parser.add_argument("--sources", nargs="+")
    parser.add_argument("--all", action="store_true", help="Run every registered source.")
    parser.add_argument("--evidence-dir", type=Path, default=Path("source-smoke-evidence"))
    return parser.parse_args(argv)


def select_sources(
    all_sources: list[str],
    *,
    batch_count: int,
    batch_index: int,
    high_risk_sources: tuple[str, ...] = HIGH_RISK_SOURCES,
) -> list[str]:
    if batch_count < 1:
        raise ValueError("batch_count 必須大於 0")
    normalized_index = batch_index % batch_count
    rotating = [source for index, source in enumerate(all_sources) if index % batch_count == normalized_index]
    return list(dict.fromkeys([*high_risk_sources, *rotating]))


def parse_json_summary(stdout: str) -> dict:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "status" in value:
            return value
    raise ValueError("找不到 news_scraper JSON summary")


def _read_latest_report(report_dir: Path) -> dict:
    reports = sorted(report_dir.glob("news_scraper_run_*.json"), reverse=True)
    if not reports:
        return {}
    try:
        value = json.loads(reports[0].read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return value if isinstance(value, dict) else {}


def _write_evidence(evidence_dir: Path, source: str, attempt: int, record: dict) -> None:
    evidence_dir.mkdir(parents=True, exist_ok=True)
    path = evidence_dir / "{}-attempt-{}.json".format(source, attempt)
    path.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def get_failure_class(record: dict) -> str:
    if record.get("timed_out"):
        return "source_outage"
    summary = record.get("summary") or {}
    report = record.get("report") or {}
    for container in (summary, report):
        diagnostics = container.get("source_diagnostics") if isinstance(container, dict) else None
        if isinstance(diagnostics, list) and diagnostics:
            value = str(diagnostics[-1].get("failure_class") or "")
            if value:
                return value
        counts = container.get("failure_class_counts") if isinstance(container, dict) else None
        if isinstance(counts, dict):
            non_zero = [name for name, count in counts.items() if count]
            if non_zero:
                return non_zero[-1]
    error_counts = summary.get("error_counts") or report.get("error_counts") or {}
    if isinstance(error_counts, dict):
        categories = {name for name, count in error_counts.items() if count}
        if categories and categories <= {"connection", "timeout"}:
            return "source_outage"
        if categories == {"ssl"}:
            return "tls_certificate"
        if categories == {"browser"}:
            return "browser_runtime"
    return "unknown"


def get_blocking_route_classes(record: dict) -> set[str]:
    report = record.get("report") or {}
    return {
        str(route.get("failure_class"))
        for route in report.get("route_attempts") or []
        if route.get("status") == "failed"
        and str(route.get("failure_class")) in BLOCKING_FAILURE_CLASSES
    }


def get_failure_host(record: dict) -> str:
    report = record.get("report") or {}
    for diagnostic in reversed(report.get("source_diagnostics") or []):
        final_route = diagnostic.get("final_route") or {}
        if final_route.get("url_host"):
            return str(final_route["url_host"])
    for attempt in reversed(report.get("source_attempts") or []):
        if attempt.get("url_host"):
            return str(attempt["url_host"])
    for route in reversed(report.get("route_attempts") or []):
        if route.get("url_host"):
            return str(route["url_host"])
        if route.get("url"):
            return urlparse(str(route["url"])).netloc.lower()
    return ""


def control_network_available(timeout=5) -> bool:
    try:
        request = Request("https://www.gov.tw/", headers={"User-Agent": "source-health-smoke"})
        with urlopen(request, timeout=timeout) as response:  # nosec B310
            return int(response.status) < 500
    except Exception:
        return False


def classify_retry_failures(records: list[dict]) -> dict[str, str]:
    classifications = {str(record["source"]): get_failure_class(record) for record in records}
    outage_hosts = {
        get_failure_host(record)
        for record in records
        if classifications.get(str(record["source"])) == "source_outage"
        and get_failure_host(record)
    }
    if len(outage_hosts) >= 3 and not control_network_available():
        for source, failure_class in list(classifications.items()):
            if failure_class == "source_outage":
                classifications[source] = "runner_network"
    return classifications


def write_health_summary(evidence_dir: Path, evidence: dict) -> None:
    classifications = evidence.get("failure_classes", {})
    warnings = sorted(
        source for source, value in classifications.items() if value in WARNING_FAILURE_CLASSES
    )
    blockers = sorted(
        source for source, value in classifications.items() if value in BLOCKING_FAILURE_CLASSES
    )
    latest_records = {}
    for record in evidence.get("attempts", []):
        latest_records[str(record.get("source") or "")] = record
    fallback_sources = sorted(
        source
        for source, record in latest_records.items()
        if any(
            bool((diagnostic.get("final_route") or {}).get("used_fallback"))
            for diagnostic in (record.get("report") or {}).get("source_diagnostics") or []
        )
    )
    coverage_reduced_sources = sorted(
        source
        for source, record in latest_records.items()
        if any(
            bool((diagnostic.get("final_route") or {}).get("coverage_reduced"))
            for diagnostic in (record.get("report") or {}).get("source_diagnostics") or []
        )
    )
    summary = {
        "schema_version": 1,
        "status": evidence["status"],
        "healthy_sources": sorted(set(evidence["selected_sources"]) - set(warnings) - set(blockers)),
        "warning_sources": warnings,
        "blocking_sources": blockers,
        "unstable_sources": evidence["unstable_sources"],
        "fallback_sources": fallback_sources,
        "coverage_reduced_sources": coverage_reduced_sources,
        "failure_classes": classifications,
    }
    (evidence_dir / "source-health-summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    lines = [
        "# Source health summary",
        "",
        "- Status: `{}`".format(summary["status"]),
        "- Healthy: {}".format("、".join(summary["healthy_sources"]) or "none"),
        "- Warnings: {}".format("、".join(warnings) or "none"),
        "- Blockers: {}".format("、".join(blockers) or "none"),
        "- Fallback used: {}".format("、".join(fallback_sources) or "none"),
        "- Reduced coverage: {}".format("、".join(coverage_reduced_sources) or "none"),
        "",
        "| Source | Classification |",
        "| --- | --- |",
    ]
    lines.extend("| {} | `{}` |".format(source, value) for source, value in sorted(classifications.items()))
    (evidence_dir / "source-health-summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_source(source: str, timeout: int, *, attempt: int, evidence_dir: Path) -> tuple[bool, str, dict]:
    started_at = datetime.now().astimezone()
    summary: dict = {}
    report: dict = {}
    stderr_tail = ""
    stdout_tail = ""
    exit_code: int | None = None
    timed_out = False
    message = ""
    artifacts_valid = False

    with tempfile.TemporaryDirectory(prefix="news-scraper-smoke-") as temp_dir:
        root = Path(temp_dir)
        command = [
            sys.executable,
            "-m",
            "news_scraper",
            "--sources",
            source,
            "--output-dir",
            str(root / "output"),
            "--report-dir",
            str(root / "reports"),
            "--max-workers",
            "1",
            "--json-summary",
            "--fail-on-source-error",
        ]
        try:
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as exc:
            timed_out = True
            stderr_tail = (exc.stderr or "")[-2000:] if isinstance(exc.stderr, str) else ""
            stdout_tail = (exc.stdout or "")[-2000:] if isinstance(exc.stdout, str) else ""
            message = "{}: smoke timeout after {} seconds".format(source, timeout)
        else:
            exit_code = completed.returncode
            stderr_tail = completed.stderr[-2000:].strip()
            stdout_tail = completed.stdout[-2000:].strip()
            report = _read_latest_report(root / "reports")
            try:
                summary = parse_json_summary(completed.stdout)
            except ValueError as exc:
                message = "{}: {} ({})".format(source, exc, stderr_tail or stdout_tail)
            else:
                failed_sources = summary.get("failed_sources") or []
                output_file = Path(str(summary.get("output_file") or ""))
                artifacts_valid = (
                    report.get("report_schema_version") == 4
                    and isinstance(report.get("source_diagnostics"), list)
                    and isinstance(report.get("route_attempts"), list)
                    and output_file.is_file()
                )
                if completed.returncode != 0 or failed_sources:
                    message = "{}: status={} failed_sources={} exit_code={}".format(
                        source,
                        summary.get("status"),
                        failed_sources,
                        completed.returncode,
                    )
                else:
                    message = "{}: status={} news_count={}".format(
                        source,
                        summary.get("status"),
                        summary.get("news_count"),
                    )

    record = {
        "source": source,
        "attempt": attempt,
        "summary": summary,
        "report": report,
    }
    blocking_route_classes = get_blocking_route_classes(record)
    success = (
        bool(summary)
        and not timed_out
        and exit_code == 0
        and not (summary.get("failed_sources") or [])
        and artifacts_valid
        and not blocking_route_classes
    )
    if summary and not artifacts_valid:
        message = "{}: schema 4 JSON evidence or Excel output is missing".format(source)
    if blocking_route_classes:
        message = "{}: blocking route evidence={}".format(
            source,
            ",".join(sorted(blocking_route_classes)),
        )
    record = {
        "source": source,
        "attempt": attempt,
        "success": success,
        "message": message,
        "started_at": started_at.isoformat(timespec="seconds"),
        "finished_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "timed_out": timed_out,
        "exit_code": exit_code,
        "summary": summary,
        "report": report,
        "stderr_tail": stderr_tail,
        "stdout_tail": stdout_tail,
    }
    _write_evidence(evidence_dir, source, attempt, record)
    return success, message, record


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    available_sources = list(SCRAPER_REGISTRY)
    if args.all:
        selected_sources = available_sources
    elif args.sources:
        unknown = [source for source in args.sources if source not in SCRAPER_REGISTRY]
        if unknown:
            raise ValueError("不支援的 smoke 來源：{}".format("、".join(unknown)))
        selected_sources = list(dict.fromkeys(args.sources))
    else:
        batch_index = args.batch_index if args.batch_index is not None else date.today().toordinal()
        selected_sources = select_sources(
            available_sources,
            batch_count=args.batch_count,
            batch_index=batch_index,
        )

    attempts: list[dict] = []
    first_failures = []
    print("Smoke sources: {}".format("、".join(selected_sources)))
    for source in selected_sources:
        success, message, record = run_source(
            source,
            timeout=args.timeout,
            attempt=1,
            evidence_dir=args.evidence_dir,
        )
        print(message)
        attempts.append(record)
        if not success:
            first_failures.append(source)

    final_failures = []
    unstable_sources = []
    retry_failure_records = []
    if first_failures:
        if args.retry_delay > 0:
            time.sleep(args.retry_delay)
        print("Retry smoke sources: {}".format("、".join(first_failures)))
        for source in first_failures:
            success, message, record = run_source(
                source,
                timeout=args.timeout,
                attempt=2,
                evidence_dir=args.evidence_dir,
            )
            print(message)
            attempts.append(record)
            if success:
                unstable_sources.append(source)
            else:
                retry_failure_records.append(record)

    failure_classes = classify_retry_failures(retry_failure_records)
    for record in retry_failure_records:
        source = str(record["source"])
        failure_class = failure_classes[source]
        if failure_class in WARNING_FAILURE_CLASSES:
            unstable_sources.append(source)
        else:
            final_failures.append(source)

    status = "failure" if final_failures else ("unstable" if unstable_sources else "success")
    evidence = {
        "schema_version": 2,
        "status": status,
        "selected_sources": selected_sources,
        "unstable_sources": unstable_sources,
        "failed_sources": final_failures,
        "failure_classes": failure_classes,
        "attempts": attempts,
    }
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    (args.evidence_dir / "source-smoke-report.json").write_text(
        json.dumps(evidence, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    write_health_summary(args.evidence_dir, evidence)

    if unstable_sources:
        print("::warning::Parser smoke unstable after retry: {}".format("、".join(unstable_sources)))
    if final_failures:
        print("::error::Parser smoke failed: {}".format("、".join(final_failures)))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
