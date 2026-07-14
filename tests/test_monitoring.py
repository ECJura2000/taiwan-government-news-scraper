import json
from datetime import datetime, timedelta, timezone

import requests

from news_scraper.monitoring import (
    RunContext,
    build_ssl_allowlist_audit,
    build_trend_summary,
    build_run_report,
    classify_error,
    detect_run_anomalies,
    load_recent_reports,
    prune_old_reports,
    send_webhook_alert,
    write_run_report,
)


def test_classify_error_distinguishes_operational_failures():
    assert classify_error(requests.Timeout("slow")) == "timeout"
    assert classify_error(requests.exceptions.SSLError("certificate failed")) == "ssl"
    assert classify_error(requests.HTTPError("503")) == "http"
    assert classify_error(ValueError("missing field")) == "parse"
    assert classify_error(RuntimeError("chromedriver failed")) == "browser"


def test_build_and_write_run_report(tmp_path):
    context = RunContext(insecure_ssl_hosts={"www.pcc.gov.tw"})
    context.record_source_attempt("財政部", 1, 3, 0.25)
    context.record_source_attempt("工程會", 1, 0, 5.0, requests.Timeout("slow"))
    context.record_source_attempt("工程會", 2, 1, 1.0)

    started_at = datetime(2026, 6, 9, 7, 0, tzinfo=timezone.utc)
    report = build_run_report(
        context=context,
        started_at=started_at,
        finished_at=started_at + timedelta(seconds=6.25),
        selected_sources=["財政部", "工程會"],
        news_count=4,
        output_path=tmp_path / "weekly.xlsx",
    )
    report_path = write_run_report(report, tmp_path / "reports")
    saved_report = json.loads(report_path.read_text(encoding="utf-8"))

    assert saved_report["status"] == "success"
    assert saved_report["error_counts"] == {"timeout": 1}
    assert saved_report["insecure_ssl_hosts"] == ["www.pcc.gov.tw"]
    assert saved_report["ai_policy"]["version"] == "2.1.0"
    assert len(saved_report["ai_policy"]["ruleset_hash"]) == 16
    assert [attempt["attempt"] for attempt in saved_report["source_attempts"]] == [1, 1, 2]


def test_parser_warning_marks_report_for_attention(tmp_path):
    context = RunContext()
    context.record_parser_warning("rss.date", "not-a-date", ValueError("invalid"), source="測試來源")
    started_at = datetime(2026, 6, 9, 7, 0, tzinfo=timezone.utc)

    report = build_run_report(
        context=context,
        started_at=started_at,
        finished_at=started_at + timedelta(seconds=1),
        selected_sources=["測試來源"],
        news_count=0,
        output_path=tmp_path / "weekly.xlsx",
    )

    assert report["status"] == "attention"
    assert report["parser_warnings"][0]["source"] == "測試來源"
    assert report["parser_warnings"][0]["error_type"] == "ValueError"


def test_detect_run_anomalies_marks_consecutive_zero_items():
    context = RunContext(quality_summary={"source_counts": {"行政院": 0}})
    previous_reports = [
        {
            "source_attempts": [
                {"source": "行政院", "status": "success", "item_count": 0},
            ]
        }
    ]

    anomalies = detect_run_anomalies(context, ["行政院"], previous_reports)

    assert anomalies[0]["category"] == "consecutive_zero_items"
    assert anomalies[0]["source"] == "行政院"
    assert anomalies[0]["threshold"] == 2


def test_detect_run_anomalies_disables_zero_check_for_configured_source():
    context = RunContext(quality_summary={"source_counts": {"農科園區": 0}})
    previous_reports = [{"quality": {"source_counts": {"農科園區": 0}}}] * 10

    assert detect_run_anomalies(context, ["農科園區"], previous_reports) == []


def test_build_trend_summary_calculates_source_success_rate():
    summary = build_trend_summary(
        [
            {
                "source_attempts": [
                    {"source": "財政部", "status": "success", "item_count": 2, "elapsed_seconds": 1.0},
                    {"source": "財政部", "status": "failed", "item_count": 0, "elapsed_seconds": 3.0},
                ],
                "quality": {"source_counts": {"財政部": 0}},
            }
        ]
    )

    assert summary["sources"]["財政部"]["success_rate"] == 0.5
    assert summary["sources"]["財政部"]["average_elapsed_seconds"] == 2.0
    assert summary["sources"]["財政部"]["zero_item_successes"] == 1


def test_ssl_allowlist_audit_lists_unused_hosts_as_removal_candidates():
    audit = build_ssl_allowlist_audit(
        [{"insecure_ssl_hosts": ["www.atp.gov.tw"]}] * 8,
        {"www.atp.gov.tw", "www.example.gov.tw"},
    )

    assert audit["status"] == "ready"
    assert audit["used_in_recent_reports"] == ["www.atp.gov.tw"]
    assert audit["removal_candidates"] == ["www.example.gov.tw"]


def test_ssl_allowlist_audit_waits_for_enough_history():
    audit = build_ssl_allowlist_audit(
        [{"insecure_ssl_hosts": ["www.atp.gov.tw"]}],
        {"www.atp.gov.tw", "www.example.gov.tw"},
    )

    assert audit["status"] == "insufficient_history"
    assert audit["removal_candidates"] == []


def test_send_webhook_alert_posts_json(monkeypatch):
    seen = {}

    class FakeResponse:
        status = 202

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, traceback):
            return False

    def fake_urlopen(request, timeout):
        seen["body"] = json.loads(request.data.decode("utf-8"))
        seen["timeout"] = timeout
        return FakeResponse()

    monkeypatch.setattr("news_scraper.monitoring.urlopen", fake_urlopen)

    result = send_webhook_alert({"status": "attention"}, "https://alerts.example.test/hook")

    assert result == {"status": "sent", "http_status": 202}
    assert seen["body"] == {"status": "attention"}


def test_send_webhook_alert_rejects_non_https_url():
    import pytest

    with pytest.raises(ValueError, match="HTTPS"):
        send_webhook_alert({"status": "attention"}, "http://alerts.example.test/hook")


def test_prune_old_reports_removes_only_expired_files(tmp_path):
    old_report = tmp_path / "news_scraper_run_old.json"
    recent_report = tmp_path / "news_scraper_run_recent.json"
    old_report.write_text("{}", encoding="utf-8")
    recent_report.write_text("{}", encoding="utf-8")
    old_timestamp = datetime(2025, 1, 1, tzinfo=timezone.utc).timestamp()
    old_report.touch()
    import os

    os.utime(old_report, (old_timestamp, old_timestamp))

    removed = prune_old_reports(tmp_path, retention_days=30, now=datetime(2026, 6, 9))

    assert removed == [str(old_report)]
    assert recent_report.exists()


def test_load_recent_reports_skips_invalid_json_schema(tmp_path):
    invalid = tmp_path / "news_scraper_run_20260613_070000_000000.json"
    valid = tmp_path / "news_scraper_run_20260613_080000_000000.json"
    invalid.write_text(json.dumps({"status": "success"}), encoding="utf-8")
    valid.write_text(
        json.dumps({"status": "success", "source_attempts": [], "quality": {}}),
        encoding="utf-8",
    )

    assert load_recent_reports(tmp_path) == [{"status": "success", "source_attempts": [], "quality": {}}]
