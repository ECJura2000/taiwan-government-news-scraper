import importlib
import json
from pathlib import Path
import subprocess
import sys
import threading

import requests

from news_scraper.application import RunResult
from news_scraper.models import NewsItem, make_news_item
from news_scraper.monitoring import RunContext

main = importlib.import_module("news_scraper.main")


def test_main_file_can_run_directly_without_shadowing_stdlib_http():
    project_root = Path(__file__).resolve().parents[1]
    completed = subprocess.run(
        [sys.executable, str(project_root / "news_scraper" / "main.py"), "--list-sources"],
        cwd=project_root,
        capture_output=True,
        text=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr
    assert "行政院" in completed.stdout


def test_main_reports_lock_contention_as_exit_4_and_json(monkeypatch, capsys, tmp_path):
    import news_scraper.application as application
    from news_scraper.run_lock import RunAlreadyActiveError

    monkeypatch.setattr(
        application,
        "run_news_scraper",
        lambda options: (_ for _ in ()).throw(RunAlreadyActiveError({"pid": 123})),
    )

    exit_code = main.main(
        [
            "--sources",
            "行政院",
            "--output-dir",
            str(tmp_path),
            "--json-summary",
        ]
    )
    captured = capsys.readouterr()
    summary = json.loads(captured.out.splitlines()[-1])

    assert exit_code == 4
    assert summary["status"] == "locked"
    assert summary["output_file"] == ""


def test_collect_all_this_week_news_keeps_affiliated_items_by_default(monkeypatch):
    items = [
        {
            "source": "內政部",
            "date": "2026-04-22",
            "department": "內政部",
            "title": "同一則新聞",
            "link": "https://example.com/a",
        },
        {
            "source": "消防署",
            "date": "2026-04-22",
            "department": "消防署",
            "title": "同一則新聞",
            "link": "https://example.com/b",
        },
    ]

    monkeypatch.setattr(main, "normalize_selected_sources", lambda selected_sources: ["內政部", "消防署"])
    monkeypatch.setattr(main, "collect_news_for_sources_once", lambda source_names, worker_count, **kwargs: (list(items), []))

    result = main.collect_all_this_week_news_concurrent(max_workers=2)

    assert len(result) == 2
    assert [item["source"] for item in result] == ["內政部", "消防署"]


def test_collect_all_this_week_news_can_still_dedupe_affiliated_items(monkeypatch):
    items = [
        {
            "source": "內政部",
            "date": "2026-04-22",
            "department": "內政部",
            "title": "同一則新聞",
            "link": "https://example.com/a",
        },
        {
            "source": "消防署",
            "date": "2026-04-22",
            "department": "消防署",
            "title": "同一則新聞",
            "link": "https://example.com/b",
        },
    ]

    monkeypatch.setattr(main, "normalize_selected_sources", lambda selected_sources: ["內政部", "消防署"])
    monkeypatch.setattr(main, "collect_news_for_sources_once", lambda source_names, worker_count, **kwargs: (list(items), []))

    result = main.collect_all_this_week_news_concurrent(max_workers=2, dedupe_affiliated=True)

    assert len(result) == 1
    assert result[0]["source"] == "消防署"


def test_normalize_selected_sources_removes_duplicates():
    assert main.normalize_selected_sources(["財政部", " 財政部 ", "法務部"]) == ["財政部", "法務部"]


def test_normalize_selected_sources_accepts_requested_official_agency_names():
    official_names = [
        "經濟部", "環境部", "原住民族委員會", "數位發展部", "文化部", "客家委員會",
        "國家發展委員會", "外交部", "行政院公共工程委員會", "國家科學及技術委員會",
        "運動部", "行政院主計總處", "教育部", "國防部", "行政院人事行政總處",
        "法務部", "財政部", "中央銀行", "內政部", "大陸委員會", "國立故宮博物院",
        "交通部", "金融監督管理委員會", "中央選舉委員會", "勞動部", "海洋委員會",
        "公平交易委員會", "農業部", "僑務委員會", "國家通訊傳播委員會",
        "衛生福利部", "國軍退除役官兵輔導委員會",
    ]

    normalized = main.normalize_selected_sources(official_names)

    assert len(normalized) == len(official_names)
    assert "人事總處" in normalized
    assert "國科會" in normalized
    assert "退輔會" in normalized

def test_build_table_data_formats_news_date_with_ad_and_roc():
    rows = main.build_table_data(
        [
            {
                "source": "數位發展部",
                "date": "2026-05-22",
                "department": "數位發展部",
                "title": "新聞",
                "link": "https://example.com/news",
            }
        ]
    )

    assert rows[0]["新聞日期"] == "2026-05-22（民國115/5/22）"


def test_order_sources_for_scraping_starts_harder_sites_first():
    assert main.order_sources_for_scraping(["國土管理署", "財政部", "公路局", "行政院"]) == [
        "國土管理署",
        "公路局",
        "行政院",
        "財政部",
    ]


def test_collect_all_this_week_news_scrape_order_does_not_change_output_order(monkeypatch):
    captured_source_names = []
    items = [
        {
            "source": "財政部",
            "date": "2026-04-22",
            "department": "財政部",
            "title": "財政部新聞",
            "link": "https://example.com/mof",
        },
        {
            "source": "國土管理署",
            "date": "2026-04-22",
            "department": "國土管理署",
            "title": "國土管理署新聞",
            "link": "https://example.com/nlma",
        },
    ]

    def fake_collect(source_names, worker_count, **kwargs):
        captured_source_names.extend(source_names)
        return list(reversed(items)), []

    monkeypatch.setattr(main, "normalize_selected_sources", lambda selected_sources: ["國土管理署", "財政部"])
    monkeypatch.setattr(main, "collect_news_for_sources_once", fake_collect)

    result = main.collect_all_this_week_news_concurrent(max_workers=2)

    assert captured_source_names == ["國土管理署", "財政部"]
    assert [item["source"] for item in result] == ["國土管理署", "財政部"]


def test_run_scraper_records_classified_failure():
    from news_scraper.monitoring import RunContext

    def fail():
        raise requests.Timeout("slow")

    context = RunContext()
    source_name, items, error = main.run_scraper("工程會", fail, log_exception=False, attempt=2, context=context)
    attempts = context.snapshot_attempts()

    assert source_name == "工程會"
    assert items == []
    assert isinstance(error, requests.Timeout)
    assert attempts[0]["attempt"] == 2
    assert attempts[0]["error_category"] == "timeout"


def test_run_scraper_isolates_selenium_timeout():
    from news_scraper.monitoring import RunContext
    from selenium.common.exceptions import TimeoutException

    context = RunContext()

    def fail():
        raise TimeoutException("page did not load")

    source_name, items, error = main.run_scraper("國土管理署", fail, log_exception=False, context=context)

    assert source_name == "國土管理署"
    assert items == []
    assert isinstance(error, TimeoutException)


def test_run_scraper_isolates_unexpected_dependency_error():
    context = RunContext()

    def fail():
        raise RuntimeError("driver transport failed")

    source_name, items, error = main.run_scraper("公路局", fail, log_exception=False, context=context)

    assert source_name == "公路局"
    assert items == []
    assert isinstance(error, RuntimeError)


def test_make_news_item_preserves_category_in_named_model():
    item = make_news_item("警政署", "警政署", "2026-06-12", "測試新聞", "https://example.com", category="新聞稿")

    assert isinstance(item, NewsItem)
    assert item.category == "新聞稿"
    assert dict(item)["category"] == "新聞稿"


def test_make_news_item_preserves_optional_summary():
    item = make_news_item(
        "國發會",
        "國發會",
        "2026-06-12",
        "測試新聞",
        "https://example.com",
        summary="摘要提到主權AI及算力建設。",
    )

    assert item.summary == "摘要提到主權AI及算力建設。"
    assert dict(item)["summary"] == "摘要提到主權AI及算力建設。"


def test_make_news_item_normalizes_html_summary_and_keeps_date_source():
    item = make_news_item(
        "主計總處",
        "主計總處",
        "2026-06-12",
        "測試新聞",
        "https://example.com",
        summary="<p>第一段&nbsp;摘要</p>\n<p>第二段</p>",
        date_source="description_fallback",
    )

    assert item.summary == "第一段 摘要 第二段"
    assert item.date_source == "description_fallback"
    assert set(dict(item)) == {
        "source", "date", "department", "title", "link", "category", "summary", "date_source",
    }


def test_collect_news_for_sources_once_isolates_parser_failure(monkeypatch):
    import news_scraper.scrapers.registry as registry

    item = {
        "source": "行政院",
        "date": "2026-07-23",
        "department": "行政院",
        "title": "成功新聞",
        "link": "https://example.test/success",
    }
    monkeypatch.setattr(
        registry,
        "SCRAPER_REGISTRY",
        {
            "行政院": lambda: [item],
            "財政部": lambda: (_ for _ in ()).throw(ValueError("fixture schema changed")),
        },
    )
    context = RunContext()
    progress = []

    items, retryable_failures = main.collect_news_for_sources_once(
        ["行政院", "財政部"],
        worker_count=1,
        context=context,
        progress_callback=lambda *args: progress.append(args),
    )

    assert items == [item]
    assert retryable_failures == []
    assert context.failed_sources == ["財政部"]
    assert [entry[0] for entry in progress] == ["行政院", "財政部"]


def test_collect_news_for_sources_once_marks_cancelled_context():
    context = RunContext()
    cancel_event = threading.Event()
    cancel_event.set()

    items, failures = main.collect_news_for_sources_once(
        ["行政院"],
        worker_count=1,
        context=context,
        cancel_event=cancel_event,
    )

    assert items == []
    assert failures == []
    assert context.cancelled is True


def test_collect_all_retries_failed_source_and_clears_timeout(monkeypatch):
    item = {
        "source": "行政院",
        "date": "2026-07-23",
        "department": "行政院",
        "title": "重試成功",
        "link": "https://example.test/retry",
    }
    calls = []

    def fake_collect(source_names, worker_count, **kwargs):
        calls.append((list(source_names), kwargs.get("attempt", 1), kwargs["context"].retry_timeout_extra_seconds))
        if len(calls) == 1:
            return [], ["行政院"]
        return [item], []

    monkeypatch.setattr(main, "normalize_selected_sources", lambda selected_sources: ["行政院"])
    monkeypatch.setattr(main, "order_sources_for_scraping", lambda source_names, **kwargs: list(source_names))
    monkeypatch.setattr(main, "collect_news_for_sources_once", fake_collect)
    context = RunContext()

    result = main.collect_all_this_week_news_concurrent(context=context, max_workers=2)

    assert result == [item]
    assert calls[0][1:] == (1, 0)
    assert calls[1][1] == 2
    assert calls[1][2] > 0
    assert context.retry_timeout_extra_seconds == 0
    assert context.failed_sources == []


def test_print_table_covers_empty_and_populated_output(capsys):
    main.print_table([], failed_sources=["榮總"])
    main.print_table(
        [
            {
                "source": "行政院",
                "date": "2026-07-23",
                "department": "行政院",
                "title": "測試新聞",
                "link": "https://example.test/news",
            }
        ]
    )

    output = capsys.readouterr().out
    assert "抓取失敗部會：榮總" in output
    assert "本週沒有抓到任何新聞" in output
    assert "全部抓取成功" in output
    assert "測試新聞" in output


def test_main_rejects_unknown_source(capsys, tmp_path):
    exit_code = main.main(["--sources", "不存在來源", "--output-dir", str(tmp_path)])

    assert exit_code == 2
    assert "不支援的來源" in capsys.readouterr().err


def test_main_reports_runtime_failure_as_json(monkeypatch, capsys, tmp_path):
    import news_scraper.application as application

    monkeypatch.setattr(
        application,
        "run_news_scraper",
        lambda options: (_ for _ in ()).throw(OSError("disk unavailable")),
    )

    exit_code = main.main(
        ["--sources", "行政院", "--output-dir", str(tmp_path), "--json-summary"]
    )
    captured = capsys.readouterr()
    summary = json.loads(captured.out.splitlines()[-1])

    assert exit_code == 1
    assert summary["status"] == "failed"
    assert "disk unavailable" in summary["error"]


def test_main_success_prints_summary_and_enforces_source_error_exit(monkeypatch, capsys, tmp_path):
    import news_scraper.application as application

    item = {
        "source": "行政院",
        "date": "2026-07-23",
        "department": "行政院",
        "title": "測試新聞",
        "link": "https://example.test/news",
    }
    result = RunResult(
        status="partial_failure",
        output_path=tmp_path / "report.xlsx",
        report_path=tmp_path / "run.json",
        news_count=1,
        failed_sources=("榮總",),
        news_items=(item,),
    )
    monkeypatch.setattr(application, "run_news_scraper", lambda options: result)

    exit_code = main.main(
        [
            "--sources",
            "行政院",
            "--output-dir",
            str(tmp_path),
            "--json-summary",
            "--fail-on-source-error",
        ]
    )
    summary = json.loads(capsys.readouterr().out.splitlines()[-1])

    assert exit_code == 3
    assert summary["status"] == "partial_failure"
    assert summary["failed_sources"] == ["榮總"]


def test_main_returns_cancelled_exit_code(monkeypatch, tmp_path):
    import news_scraper.application as application

    monkeypatch.setattr(
        application,
        "run_news_scraper",
        lambda options: RunResult(
            status="cancelled",
            output_path=None,
            report_path=None,
            news_count=0,
            cancelled=True,
        ),
    )

    assert main.main(["--sources", "行政院", "--output-dir", str(tmp_path)]) == 130
