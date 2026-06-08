from news_scraper.utils.dedupe import dedupe_affiliated_news


def test_dedupe_affiliated_news_prefers_child_agency():
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

    deduped = dedupe_affiliated_news(items)
    assert len(deduped) == 1
    assert deduped[0]["source"] == "消防署"
