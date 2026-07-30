import importlib.util
import json
from pathlib import Path


def load_smoke_module():
    path = Path(__file__).resolve().parents[1] / "scripts" / "smoke_sources.py"
    spec = importlib.util.spec_from_file_location("smoke_sources", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_select_sources_keeps_high_risk_and_rotates_all_sources():
    smoke = load_smoke_module()
    all_sources = ["行政院", "監察院", "司法院", "財政部", "榮總", "工程會"]

    first = smoke.select_sources(all_sources, batch_count=2, batch_index=0)
    second = smoke.select_sources(all_sources, batch_count=2, batch_index=1)

    assert {"榮總", "司法院", "財政部"} <= set(first)
    assert {"榮總", "司法院", "財政部"} <= set(second)
    assert set(first) | set(second) == set(all_sources)


def test_parse_json_summary_uses_last_machine_readable_line():
    smoke = load_smoke_module()

    summary = smoke.parse_json_summary('log line\n{"status":"success","news_count":2}\n')

    assert summary == {"status": "success", "news_count": 2}


def test_main_retries_failed_source_and_marks_it_unstable(monkeypatch, tmp_path):
    smoke = load_smoke_module()
    outcomes = iter(
        [
            (False, "first failed", {"source": "行政院", "attempt": 1}),
            (True, "retry passed", {"source": "行政院", "attempt": 2}),
        ]
    )
    monkeypatch.setattr(smoke, "run_source", lambda *_args, **_kwargs: next(outcomes))

    exit_code = smoke.main(
        [
            "--sources",
            "行政院",
            "--retry-delay",
            "0",
            "--evidence-dir",
            str(tmp_path),
        ]
    )
    report = json.loads((tmp_path / "source-smoke-report.json").read_text(encoding="utf-8"))

    assert exit_code == 0
    assert report["status"] == "unstable"
    assert report["unstable_sources"] == ["行政院"]
    assert report["failed_sources"] == []
    assert [attempt["attempt"] for attempt in report["attempts"]] == [1, 2]


def test_main_fails_only_after_retry_still_fails(monkeypatch, tmp_path):
    smoke = load_smoke_module()
    outcomes = iter(
        [
            (False, "first failed", {"source": "行政院", "attempt": 1}),
            (False, "retry failed", {"source": "行政院", "attempt": 2}),
        ]
    )
    monkeypatch.setattr(smoke, "run_source", lambda *_args, **_kwargs: next(outcomes))

    exit_code = smoke.main(
        [
            "--sources",
            "行政院",
            "--retry-delay",
            "0",
            "--evidence-dir",
            str(tmp_path),
        ]
    )
    report = json.loads((tmp_path / "source-smoke-report.json").read_text(encoding="utf-8"))

    assert exit_code == 1
    assert report["status"] == "failure"
    assert report["failed_sources"] == ["行政院"]


def test_main_marks_transient_network_retry_failure_as_unstable(monkeypatch, tmp_path):
    smoke = load_smoke_module()
    outcomes = iter(
        [
            (
                False,
                "first failed",
                {
                    "source": "社家署",
                    "attempt": 1,
                    "summary": {
                        "error_counts": {"connection": 2},
                    },
                    "stderr_tail": "Network is unreachable",
                    "stdout_tail": "",
                },
            ),
            (
                False,
                "retry failed",
                {
                    "source": "社家署",
                    "attempt": 2,
                    "summary": {
                        "error_counts": {"connection": 2},
                    },
                    "stderr_tail": "Failed to establish a new connection",
                    "stdout_tail": "",
                },
            ),
        ]
    )
    monkeypatch.setattr(smoke, "run_source", lambda *_args, **_kwargs: next(outcomes))

    exit_code = smoke.main(
        [
            "--sources",
            "社家署",
            "--retry-delay",
            "0",
            "--evidence-dir",
            str(tmp_path),
        ]
    )
    report = json.loads((tmp_path / "source-smoke-report.json").read_text(encoding="utf-8"))

    assert exit_code == 0
    assert report["status"] == "unstable"
    assert report["unstable_sources"] == ["社家署"]
    assert report["failed_sources"] == []


def test_write_evidence_preserves_report_and_stderr(tmp_path):
    smoke = load_smoke_module()
    record = {
        "source": "行政院",
        "attempt": 1,
        "report": {"error_counts": {"http": 1}},
        "stderr_tail": "HTTP 503",
    }

    smoke._write_evidence(tmp_path, "行政院", 1, record)
    saved = json.loads((tmp_path / "行政院-attempt-1.json").read_text(encoding="utf-8"))

    assert saved["report"]["error_counts"] == {"http": 1}
    assert saved["stderr_tail"] == "HTTP 503"


def test_get_failure_class_uses_structured_error_counts():
    smoke = load_smoke_module()
    record = {
        "summary": {"error_counts": {"connection": 2}},
        "stderr_tail": "HTTPSConnectionPool host='www.sfaa.gov.tw' Network is unreachable",
        "stdout_tail": "",
    }

    assert smoke.get_failure_class(record) == "source_outage"


def test_subprocess_timeout_is_source_outage():
    smoke = load_smoke_module()

    assert smoke.get_failure_class({"timed_out": True}) == "source_outage"


def test_route_parser_regression_blocks_release_even_when_fallback_succeeds():
    smoke = load_smoke_module()
    record = {
        "report": {
            "route_attempts": [
                {
                    "route_id": "primary",
                    "status": "failed",
                    "failure_class": "parser_regression",
                },
                {
                    "route_id": "fallback",
                    "status": "success",
                    "failure_class": "",
                },
            ]
        }
    }

    assert smoke.get_blocking_route_classes(record) == {"parser_regression"}


def test_classify_retry_failures_requires_three_hosts_and_failed_control_probe(monkeypatch):
    smoke = load_smoke_module()
    records = [
        {
            "source": source,
            "report": {
                "source_diagnostics": [
                    {
                        "source": source,
                        "failure_class": "source_outage",
                        "final_route": {"url_host": host},
                    }
                ]
            },
        }
        for source, host in (
            ("行政院", "www.ey.gov.tw"),
            ("財政部", "www.mof.gov.tw"),
            ("社家署", "www.sfaa.gov.tw"),
        )
    ]
    monkeypatch.setattr(smoke, "control_network_available", lambda: False)

    assert set(smoke.classify_retry_failures(records).values()) == {"runner_network"}
