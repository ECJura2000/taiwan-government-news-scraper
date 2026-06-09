from datetime import date
from pathlib import Path

from news_scraper.rss.parser import extract_rss_item_metadata_fields, parse_rss_items
from news_scraper.scrapers.ministry.health import cdc
from news_scraper.scrapers.ministry.interior import nlma
from news_scraper.scrapers.ministry.judicial import judicial_yuan
from news_scraper.scrapers.ministry.transport import motc
from news_scraper.scrapers.base import make_soup

FIXTURES = Path(__file__).parent / "fixtures"


def test_rss_fixture_preserves_core_fields():
    xml_text = (FIXTURES / "sample_rss.xml").read_text(encoding="utf-8")
    items = parse_rss_items(xml_text, "https://example.gov.tw/rss")
    fields = extract_rss_item_metadata_fields(items[0])

    assert fields["title"] == "測試機關發布新聞"
    assert fields["link"] == "https://example.gov.tw/news/1"
    assert fields["author"] == "測試機關"


def test_cdc_fixture_preserves_title_date_and_link(monkeypatch):
    html = (FIXTURES / "cdc_list.html").read_text(encoding="utf-8")
    monkeypatch.setattr(cdc, "fetch_html", lambda *args, **kwargs: html)
    monkeypatch.setattr(cdc, "get_cached_week_range", lambda: (date(2026, 6, 8), date(2026, 6, 14)))

    items = cdc.scrape_cdc_this_week()

    assert items == [
        {
            "source": "疾管署",
            "date": "2026-06-09",
            "department": "疾管署",
            "title": "測試防疫新聞",
            "link": "https://www.cdc.gov.tw/Bulletin/Detail/example?typeid=9",
        }
    ]


def test_judicial_fixture_preserves_department_date_and_link():
    html = (FIXTURES / "judicial_list.html").read_text(encoding="utf-8")
    row = make_soup(html).select_one("tr")

    news_date, item = judicial_yuan.extract_judicial_yuan_row(row, "司法院")

    assert news_date == date(2026, 6, 9)
    assert item["department"] == "司法院／臺灣臺南地方法院"
    assert item["link"] == "https://www.judicial.gov.tw/tw/cp-1888-example.html"


def test_motc_fixture_preserves_department_date_and_link(monkeypatch):
    html = (FIXTURES / "motc_list.html").read_text(encoding="utf-8")
    monkeypatch.setattr(motc, "fetch_html", lambda *args, **kwargs: html)
    monkeypatch.setattr(motc, "get_cached_week_range", lambda: (date(2026, 6, 8), date(2026, 6, 14)))

    items = motc.scrape_motc_this_week()

    assert items[0]["date"] == "2026-06-09"
    assert items[0]["department"] == "交通部／公共運輸及監理司"
    assert "serno=test" in items[0]["link"]


def test_nlma_fixture_preserves_department_date_and_link(monkeypatch):
    html = (FIXTURES / "nlma_list.html").read_text(encoding="utf-8")

    class FakeDriver:
        page_source = html

        def get(self, url):
            return None

        def quit(self):
            return None

    class FakeWait:
        def __init__(self, *args, **kwargs):
            pass

        def until(self, condition):
            return True

    monkeypatch.setattr(nlma, "create_selenium_driver", FakeDriver)
    monkeypatch.setattr(nlma, "WebDriverWait", FakeWait)
    monkeypatch.setattr(nlma.time, "sleep", lambda seconds: None)
    monkeypatch.setattr(nlma, "get_cached_week_range", lambda: (date(2026, 6, 8), date(2026, 6, 14)))

    items = nlma.scrape_nlma_this_week()

    assert items[0]["date"] == "2026-06-09"
    assert items[0]["department"] == "國土管理署／下水道建設組"
    assert items[0]["link"] == "https://www.nlma.gov.tw/ch/titlelist/news/example"
