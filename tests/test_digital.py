from datetime import date

from news_scraper.scrapers.ministry.digital import moda
from news_scraper.utils import dates


def test_scrape_adi_this_week_parses_moda_list(monkeypatch):
    html = """
    <ul id="ListTable">
      <li>
        <a href="/ADI/news/latest-news/19724.html">
          <span class="listDate">2026-05-19</span>
          <span class="listUnit">推動AI產業發展</span>
          <span class="title5">數發部AI 產業人才認定指引升級3.0</span>
        </a>
      </li>
      <li>
        <a href="/ADI/news/latest-news/19576.html">
          <span class="listDate">2026-04-29</span>
          <span class="title5">過舊新聞</span>
        </a>
      </li>
    </ul>
    """
    monkeypatch.setattr(dates, "CURRENT_WEEK_RANGE", (date(2026, 5, 18), date(2026, 5, 24)))
    monkeypatch.setattr(moda, "fetch_html", lambda url: html)

    assert moda.scrape_adi_this_week() == [
        {
            "source": "數位產業署",
            "date": "2026-05-19",
            "department": "數位產業署／推動AI產業發展",
            "title": "數發部AI 產業人才認定指引升級3.0",
            "link": "https://moda.gov.tw/ADI/news/latest-news/19724.html",
        }
    ]


def test_scrape_acs_this_week_uses_source_when_unit_missing(monkeypatch):
    html = """
    <ul id="ListTable">
      <li>
        <a href="/ACS/press/news/press/19726.html">
          <span class="listDate">2026-05-21</span>
          <span class="title5">資安署啟動重要資服業者聯合稽核</span>
        </a>
      </li>
    </ul>
    """
    monkeypatch.setattr(dates, "CURRENT_WEEK_RANGE", (date(2026, 5, 18), date(2026, 5, 24)))
    monkeypatch.setattr(moda, "fetch_html", lambda url: html)

    assert moda.scrape_acs_this_week()[0]["department"] == "資通安全署"


def test_scrape_nics_this_week_parses_next_data(monkeypatch):
    html = """
    <script id="__NEXT_DATA__" type="application/json">
    {
      "props": {
        "pageProps": {
          "data": {
            "content": [
              {
                "data": {
                  "item": [
                    {
                      "label": "聚焦 CYBERSEC 2026：資安院展現全民資安推廣與 OT 防禦成果",
                      "link": "/latest_news/announcements/Latest_Announcement/abc",
                      "date": "2026/5/22"
                    },
                    {
                      "label": "過舊新聞",
                      "link": "/latest_news/announcements/Latest_Announcement/old",
                      "date": "2026/4/28"
                    }
                  ]
                }
              }
            ]
          }
        }
      }
    }
    </script>
    """
    monkeypatch.setattr(dates, "CURRENT_WEEK_RANGE", (date(2026, 5, 18), date(2026, 5, 24)))
    monkeypatch.setattr(moda, "fetch_html_plain_insecure", lambda url: html)

    assert moda.scrape_nics_this_week() == [
        {
            "source": "國家資通安全研究院",
            "date": "2026-05-22",
            "department": "國家資通安全研究院",
            "title": "聚焦 CYBERSEC 2026：資安院展現全民資安推廣與 OT 防禦成果",
            "link": "https://www.nics.nat.gov.tw/latest_news/announcements/Latest_Announcement/abc",
        }
    ]
