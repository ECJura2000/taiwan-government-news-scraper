from datetime import date

from news_scraper.scrapers.ministry.development import niar


def test_scrape_niar_this_week_parses_table_rows(monkeypatch):
    monkeypatch.setattr(
        "news_scraper.scrapers.ministry.development.niar.get_cached_week_range",
        lambda: (date(2026, 4, 27), date(2026, 5, 3)),
    )
    monkeypatch.setattr(
        "news_scraper.scrapers.ministry.development.niar.fetch_html",
        lambda *args, **kwargs: """
        <table class="rwdTable">
          <tr><th class="date">新聞日期</th><th class="title">標題</th></tr>
          <tr>
            <td class="date">2026-04-28</td>
            <td class="title">
              <a href="/xmdoc/cont?xsmsid=0I148622737263495777&amp;sid=abc" title="國研院新聞">國研院新聞</a>
            </td>
          </tr>
          <tr>
            <td class="date">2026-04-10</td>
            <td class="title"><a href="/old" title="舊聞">舊聞</a></td>
          </tr>
        </table>
        """,
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
