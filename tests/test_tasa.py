import json
from datetime import date
from pathlib import Path

import requests

from news_scraper.scrapers.ministry.development import tasa

FIXTURES = Path(__file__).parent / "fixtures"


def test_parse_tasa_publish_date_converts_utc_to_taiwan_date():
    assert tasa.parse_tasa_publish_date("2026-04-27T16:30:00.000Z") == date(2026, 4, 28)


def test_build_tasa_item_uses_zh_title_and_detail_link():
    item = tasa.build_tasa_item(
        {
            "id": "6de39df9-01aa-4655-b4c7-275e0787337e",
            "title": {"ZH_TW": "率隊參與Space Symposium 盛會", "EN_US": "Space Symposium"},
        },
        date(2026, 4, 28),
    )

    assert item == {
        "source": "國家太空中心",
        "date": "2026-04-28",
        "department": "國家太空中心",
        "title": "率隊參與Space Symposium 盛會",
        "link": "https://www.tasa.org.tw/zh-TW/announcements/detail/6de39df9-01aa-4655-b4c7-275e0787337e",
    }


def test_tasa_graphql_fixture_preserves_api_contract():
    payload = json.loads((FIXTURES / "tasa_graphql.json").read_text(encoding="utf-8"))
    row = payload["data"]["findAnnouncements"]["items"][0]
    news_date = tasa.parse_tasa_publish_date(row["publishAt"])

    assert news_date == date(2026, 4, 28)
    assert tasa.build_tasa_item(row, news_date)["title"] == "率隊參與Space Symposium 盛會"


def test_fetch_tasa_announcements_falls_back_to_curl(monkeypatch):
    monkeypatch.setattr(
        tasa,
        "fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(requests.ConnectionError("TLS reset")),
    )
    calls = []

    def fake_curl(url, **kwargs):
        calls.append((url, kwargs))
        return json.dumps({"data": {"findAnnouncements": {"total": 0, "items": []}}})

    monkeypatch.setattr(tasa, "fetch_html_by_curl_with_headers", fake_curl)

    assert tasa.fetch_tasa_announcements() == {"total": 0, "items": []}
    assert calls[0][1]["method"] == "POST"
    assert calls[0][1]["data"]
