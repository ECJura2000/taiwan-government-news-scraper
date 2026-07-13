import importlib
import requests

from news_scraper.models import NewsItem, make_news_item

main = importlib.import_module("news_scraper.main")


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
    from news_scraper.monitoring import RunContext

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
