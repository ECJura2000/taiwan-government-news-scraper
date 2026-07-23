from datetime import date
from pathlib import Path

import pytest
import requests

from news_scraper.errors import CurlRequestError, DownloadError
from news_scraper.scrapers.ministry.veterans import vghtpe

FIXTURES = Path(__file__).parent / "fixtures"


def test_parse_vghtpe_list_fixture(monkeypatch):
    monkeypatch.setattr(vghtpe, "get_cached_week_range", lambda: (date(2026, 7, 20), date(2026, 7, 26)))

    items = vghtpe.parse_vghtpe_list_html((FIXTURES / "vghtpe_list.html").read_text(encoding="utf-8"))

    assert len(items) == 1
    assert items[0].title == "北榮精準醫療新成果"
    assert items[0].date == "2026-07-22"
    assert items[0].link.endswith("News!one.action?nid=20001&gcode=A05")


def test_parse_vghtpe_home_fixture(monkeypatch):
    monkeypatch.setattr(vghtpe, "get_cached_week_range", lambda: (date(2026, 7, 20), date(2026, 7, 26)))

    items = vghtpe.parse_vghtpe_home_html((FIXTURES / "vghtpe_home.html").read_text(encoding="utf-8"))

    assert len(items) == 1
    assert items[0].department == "榮總／新聞稿"
    assert items[0].title == "臺北榮總跨域醫療合作"


def test_cloudflare_challenge_uses_selenium(monkeypatch):
    monkeypatch.setattr(vghtpe, "fetch_vghtpe_http", lambda *args, **kwargs: "<html>Just a moment cf-chl-test</html>")
    monkeypatch.setattr(vghtpe, "fetch_html_by_selenium", lambda *args, **kwargs: "<html><table></table></html>")

    assert vghtpe.load_vghtpe_page("https://example.test/news", "table") == "<html><table></table></html>"


def test_cloudflare_403_uses_selenium(monkeypatch):
    monkeypatch.setattr(
        vghtpe,
        "fetch_vghtpe_http",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            CurlRequestError(
                "HTTP 403",
                url="https://example.test/news",
                exit_code=22,
                http_status=403,
                error_category="http",
            )
        ),
    )
    monkeypatch.setattr(vghtpe, "fetch_html_by_selenium", lambda *args, **kwargs: "<html>loaded</html>")

    assert vghtpe.load_vghtpe_page("https://example.test/news", "table") == "<html>loaded</html>"


def test_cf_mitigated_header_is_detected(monkeypatch):
    class FakeResponse:
        status_code = 200
        headers = {"cf-mitigated": "challenge"}
        text = "<html>challenge</html>"

    monkeypatch.setattr(vghtpe, "fetch_response", lambda *args, **kwargs: FakeResponse())

    with pytest.raises(DownloadError, match="cf-mitigated"):
        vghtpe.fetch_vghtpe_http("https://example.test/news")


def test_http_403_is_classified_as_cloudflare_challenge(monkeypatch):
    response = requests.Response()
    response.status_code = 403
    error = requests.HTTPError("forbidden", response=response)
    monkeypatch.setattr(
        vghtpe,
        "fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(error),
    )

    with pytest.raises(DownloadError, match="HTTP 403 Cloudflare"):
        vghtpe.fetch_vghtpe_http("https://example.test/news")


def test_vghtpe_http_uses_secure_curl_after_request_error(monkeypatch):
    monkeypatch.setattr(
        vghtpe,
        "fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(requests.Timeout("slow")),
    )
    monkeypatch.setattr(
        vghtpe,
        "fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: "<html>curl</html>",
    )

    assert vghtpe.fetch_vghtpe_http("https://example.test/news") == "<html>curl</html>"


def test_load_vghtpe_page_reports_both_failures(monkeypatch):
    monkeypatch.setattr(
        vghtpe,
        "fetch_vghtpe_http",
        lambda *args, **kwargs: (_ for _ in ()).throw(TimeoutError("requests timeout")),
    )
    monkeypatch.setattr(
        vghtpe,
        "fetch_html_by_selenium",
        lambda *args, **kwargs: (_ for _ in ()).throw(RuntimeError("browser unavailable")),
    )

    with pytest.raises(DownloadError, match="browser unavailable"):
        vghtpe.load_vghtpe_page("https://example.test/news", "table")


def test_scraper_falls_back_to_homepage(monkeypatch):
    monkeypatch.setattr(vghtpe, "get_cached_week_range", lambda: (date(2026, 7, 20), date(2026, 7, 26)))
    home_html = (FIXTURES / "vghtpe_home.html").read_text(encoding="utf-8")
    pages = iter(["<html><table></table></html>", home_html])
    monkeypatch.setattr(vghtpe, "load_vghtpe_page", lambda *args, **kwargs: next(pages))

    items = vghtpe.scrape_vghtpe_this_week()

    assert [item.title for item in items] == ["臺北榮總跨域醫療合作"]
