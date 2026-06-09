from datetime import date
from pathlib import Path

from news_scraper.scrapers.ministry.development import niar

FIXTURES = Path(__file__).parent / "fixtures"


def test_scrape_niar_this_week_parses_table_rows(monkeypatch):
    monkeypatch.setattr(
        "news_scraper.scrapers.ministry.development.niar.get_cached_week_range",
        lambda: (date(2026, 4, 27), date(2026, 5, 3)),
    )
    monkeypatch.setattr(
        "news_scraper.scrapers.ministry.development.niar.fetch_html",
        lambda *args, **kwargs: (FIXTURES / "niar_list.html").read_text(encoding="utf-8"),
    )

    assert niar.scrape_niar_this_week() == [
        {
            "source": "國家實驗研究院",
            "date": "2026-04-28",
            "department": "國家實驗研究院",
            "title": "國研院新聞",
            "link": "https://www.niar.org.tw/xmdoc/cont?xsmsid=0I148622737263495777&sid=abc",
        }
    ]
