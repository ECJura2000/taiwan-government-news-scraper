from news_scraper.monitoring import RunContext, use_run_context
from news_scraper.utils.dates import ad_date_to_roc_compact_str, format_ad_and_roc_date, parse_rss_pubdate, roc_to_ad_date


def test_roc_to_ad_date():
    assert str(roc_to_ad_date("115-04-22")) == "2026-04-22"


def test_ad_date_to_roc_compact_str():
    assert ad_date_to_roc_compact_str(roc_to_ad_date("115-04-22")) == "1150422"


def test_format_ad_and_roc_date_keeps_both_calendars():
    assert format_ad_and_roc_date("2026-04-22") == "2026-04-22（民國115/4/22）"


def test_parse_rss_pubdate_converts_utc_to_taipei_before_taking_date():
    assert str(parse_rss_pubdate("Wed, 24 Jun 2026 16:00:00 GMT")) == "2026-06-25"
    assert str(parse_rss_pubdate("2026-06-24T16:30:00Z")) == "2026-06-25"


def test_parse_rss_pubdate_keeps_naive_calendar_dates_unchanged():
    assert str(parse_rss_pubdate("2026-06-24")) == "2026-06-24"


def test_matched_but_invalid_rss_date_is_observable():
    context = RunContext()

    with use_run_context(context):
        assert parse_rss_pubdate("2026-99-99") is None

    assert context.parser_warnings[0]["parser"] == "parse_rss_pubdate.ad"
