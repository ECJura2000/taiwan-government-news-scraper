from datetime import datetime, timezone

from news_scraper.monitoring import RunContext, build_run_report, should_send_alert
from news_scraper.quality import normalize_url, process_news_quality


def make_item(title, link, source="疾管署", news_date="2026-06-09"):
    return {
        "source": source,
        "date": news_date,
        "department": source,
        "title": title,
        "link": link,
    }


def test_normalize_url_removes_tracking_and_fragment():
    assert normalize_url("HTTPS://Example.COM/news?id=1&utm_source=rss#top") == "https://example.com/news?id=1"


def test_process_news_quality_dedupes_and_excludes_non_news():
    items = [
        make_item("同一則防疫新聞", "https://example.com/a"),
        make_item("同一則防疫新聞", "https://example.com/b"),
        make_item("電機技士-行政院人事行政總處事求人機關徵才系統", "https://example.com/job", source="農科園區"),
        make_item("網址錯誤", "not-a-url"),
    ]

    cleaned, summary = process_news_quality(items, ["疾管署", "農科園區"])

    assert len(cleaned) == 1
    assert summary["duplicate_count"] == 1
    assert summary["excluded_non_news_count"] == 1
    assert summary["invalid_count"] == 1
    assert summary["source_counts"] == {"疾管署": 1, "農科園區": 0}
    assert summary["alert_reasons"] == ["invalid_items"]


def test_quality_alert_ignores_small_normal_cleanup():
    items = [
        make_item("新聞一", "https://example.com/a"),
        make_item("新聞一", "https://example.com/b"),
        make_item("電機技士-行政院人事行政總處事求人機關徵才系統", "https://example.com/job", source="農科園區"),
    ]

    _, summary = process_news_quality(items, ["疾管署", "農科園區"])

    assert summary["alert_reasons"] == []

    context = RunContext(quality_summary=summary)
    report = build_run_report(
        context=context,
        started_at=datetime(2026, 6, 9, tzinfo=timezone.utc),
        finished_at=datetime(2026, 6, 9, tzinfo=timezone.utc),
        selected_sources=["疾管署", "農科園區"],
        news_count=1,
        output_path="weekly.xlsx",
    )
    assert report["status"] == "success"
    assert should_send_alert(report) is False


def test_quality_alert_marks_duplicate_spike():
    items = [make_item("新聞{}".format(index), "https://example.com/{}".format(index)) for index in range(10)]
    items.extend(make_item("新聞0", "https://example.com/duplicate/{}".format(index)) for index in range(5))

    _, summary = process_news_quality(items, ["疾管署"])

    assert summary["duplicate_count"] == 5
    assert summary["alert_reasons"] == ["duplicate_spike"]
