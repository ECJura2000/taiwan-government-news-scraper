import xml.etree.ElementTree as ET
from datetime import date

from news_scraper.monitoring import RunContext, use_run_context
from news_scraper.rss.parser import resolve_rss_news_date_with_source
from news_scraper.scrapers import base


def make_item(title, pub_date):
    return ET.fromstring(
        "<item><title>{}</title><link>https://example.gov.tw/{}</link><pubDate>{}</pubDate></item>".format(
            title,
            title,
            pub_date,
        )
    )


def test_standard_rss_does_not_drop_current_items_after_an_old_item(monkeypatch):
    monkeypatch.setattr(base, "get_cached_week_range", lambda: (date(2026, 6, 22), date(2026, 6, 28)))
    monkeypatch.setattr(
        base,
        "fetch_rss_items",
        lambda *args, **kwargs: [
            make_item("本週一", "2026-06-24"),
            make_item("舊聞", "2026-06-01"),
            make_item("本週二", "2026-06-25"),
        ],
    )

    items = base.scrape_standard_rss_this_week("測試機關", "https://example.gov.tw/rss")

    assert [item.title for item in items] == ["本週一", "本週二"]


def test_description_date_fallback_is_labeled_and_marks_attention():
    context = RunContext()
    with use_run_context(context):
        news_date, date_source = resolve_rss_news_date_with_source(
            {
                "pub_date_text": "",
                "date_source": "",
                "description": "發布日期：2026-06-24；活動日期：2026-07-01",
            },
            allow_description_fallback=True,
            source="測試機關",
        )

    assert news_date == date(2026, 6, 24)
    assert date_source == "description_fallback"
    assert context.parser_warnings[0]["parser"] == "rss.date.description_fallback"
    assert context.parser_warnings[0]["source"] == "測試機關"


def test_updated_rss_field_preserves_date_provenance():
    item = ET.fromstring("<item><updated>2026-06-24T16:00:00Z</updated></item>")
    date_fields = base.extract_rss_item_date_fields(item)
    news_date, date_source = resolve_rss_news_date_with_source(date_fields)

    assert news_date == date(2026, 6, 25)
    assert date_source == "updated"


def test_detail_summary_extraction_returns_plain_text(monkeypatch):
    html = """
    <div class="article1 cpArticle">
      <p>第一段&nbsp;摘要</p><script>ignore()</script><p>第二段摘要</p>
    </div>
    """
    monkeypatch.setattr(base, "fetch_html", lambda *args, **kwargs: html)

    summary = base.fetch_page_summary("https://example.gov.tw/news", (".article1.cpArticle",))

    assert summary == "第一段 摘要 第二段摘要"
