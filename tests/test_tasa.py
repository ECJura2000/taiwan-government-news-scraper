from datetime import date

from news_scraper.scrapers.ministry.development import tasa


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
