import xml.etree.ElementTree as ET
from datetime import date

from news_scraper.scrapers.ministry.executive import dgpa


def make_rss_item(title, link, pub_date):
    return ET.fromstring(
        "<item><title>{}</title><link>{}</link><pubDate>{}</pubDate></item>".format(
            title,
            link.replace("&", "&amp;"),
            pub_date,
        )
    )


def test_scrape_dgpa_converts_utc_rss_date_to_taipei_date(monkeypatch):
    monkeypatch.setattr(
        dgpa,
        "get_cached_week_range",
        lambda: (date(2026, 6, 22), date(2026, 6, 28)),
    )
    monkeypatch.setattr(
        dgpa,
        "fetch_rss_items",
        lambda *args, **kwargs: [
            make_rss_item(
                "人事總處AI人才新聞",
                "https://www.dgpa.gov.tw/information?uid=82&pid=13015",
                "Wed, 24 Jun 2026 16:00:00 GMT",
            ),
        ],
    )

    items = dgpa.scrape_dgpa_this_week()

    assert len(items) == 1
    assert items[0]["source"] == "人事總處"
    assert items[0]["date"] == "2026-06-25"
