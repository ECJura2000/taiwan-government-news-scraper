from datetime import date
from importlib import import_module
from pathlib import Path

import pytest
import requests

from news_scraper.errors import ParserContractError
from news_scraper.models import make_news_item
from news_scraper.source_catalog import get_source_routes

FIXTURE_ROOT = Path(__file__).parent / "fixtures" / "scrapers"
WEEK_RANGE = (date(2026, 7, 27), date(2026, 8, 2))
CASES = (
    (
        "news_scraper.scrapers.ministry.justice.mjac",
        "scrape_mjac_this_week",
        "fetch_html_resilient",
        "mjac.html",
        "矯正署",
        "矯正署本週新聞",
        "矯正署",
    ),
    (
        "news_scraper.scrapers.ministry.agriculture.afna",
        "scrape_afna_this_week",
        "fetch_html_resilient",
        "afna.html",
        "農業金融署",
        "農業金融署本週新聞",
        "農業金融署",
    ),
    (
        "news_scraper.scrapers.ministry.agriculture.fa",
        "scrape_fa_this_week",
        "fetch_html_resilient",
        "fa.html",
        "漁業署",
        "漁業署本週新聞",
        "漁業署",
    ),
    (
        "news_scraper.scrapers.ministry.health.sfaa",
        "scrape_sfaa_this_week",
        "fetch_html_resilient",
        "sfaa.html",
        "社家署",
        "社家署本週新聞",
        "社家署／老人福利組",
    ),
    (
        "news_scraper.scrapers.ministry.environment.moenv",
        "scrape_moenv_this_week",
        "fetch_html_resilient",
        "moenv.html",
        "環境部",
        "環境部本週新聞",
        "環境部",
    ),
    (
        "news_scraper.scrapers.ministry.regulators.pcc",
        "scrape_pcc_this_week",
        "fetch_html_resilient",
        "pcc.html",
        "工程會",
        "工程會本週新聞",
        "工程會",
    ),
)


@pytest.mark.parametrize(
    ("module_name", "function_name", "fetch_name", "fixture_name", "source", "title", "department"),
    CASES,
)
def test_high_risk_scraper_parses_stable_fixture(
    monkeypatch,
    module_name,
    function_name,
    fetch_name,
    fixture_name,
    source,
    title,
    department,
):
    module = import_module(module_name)
    html = (FIXTURE_ROOT / fixture_name).read_text(encoding="utf-8")
    monkeypatch.setattr(module, fetch_name, lambda *_args, **_kwargs: html)
    monkeypatch.setattr(module, "get_cached_week_range", lambda: WEEK_RANGE)

    results = getattr(module, function_name)()

    assert len(results) == 1
    assert results[0]["source"] == source
    assert results[0]["title"] == title
    assert results[0]["department"] == department
    assert results[0]["date"] == "2026-07-29"
    assert results[0]["link"].startswith("http")


@pytest.mark.parametrize(
    ("module_name", "function_name", "fetch_name"),
    [(case[0], case[1], case[2]) for case in CASES],
)
def test_high_risk_scraper_rejects_missing_list_structure(
    monkeypatch,
    module_name,
    function_name,
    fetch_name,
):
    module = import_module(module_name)
    monkeypatch.setattr(module, fetch_name, lambda *_args, **_kwargs: "<html></html>")
    source = next(case[4] for case in CASES if case[0] == module_name)
    monkeypatch.setattr(module, "get_source_routes", lambda _source: (get_source_routes(source)[0],))

    with pytest.raises(ParserContractError, match="找不到"):
        getattr(module, function_name)()


@pytest.mark.parametrize(
    ("module_name", "function_name", "source", "title"),
    (
        (
            "news_scraper.scrapers.ministry.agriculture.fa",
            "scrape_fa_this_week",
            "漁業署",
            "漁業署 RSS 備援新聞",
        ),
        (
            "news_scraper.scrapers.ministry.agriculture.afna",
            "scrape_afna_this_week",
            "農業金融署",
            "農業金融署 RSS 備援新聞",
        ),
        (
            "news_scraper.scrapers.ministry.regulators.pcc",
            "scrape_pcc_this_week",
            "工程會",
            "工程會開放資料備援新聞",
        ),
    ),
)
def test_official_feed_fallback_is_used_after_primary_failure(
    monkeypatch,
    module_name,
    function_name,
    source,
    title,
):
    module = import_module(module_name)
    monkeypatch.setattr(
        module,
        "fetch_html_resilient",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(requests.Timeout("primary unavailable")),
    )
    monkeypatch.setattr(
        module,
        "scrape_standard_rss_this_week",
        lambda *_args, **_kwargs: [
            make_news_item(
                source,
                source,
                date(2026, 7, 29),
                title,
                "https://example.test/fallback",
            )
        ],
    )

    results = getattr(module, function_name)()

    assert [item["title"] for item in results] == [title]


def test_moenv_browser_fallback_parses_fixed_fixture(monkeypatch):
    module = import_module("news_scraper.scrapers.ministry.environment.moenv")
    portal_html = (FIXTURE_ROOT / "moenv_portal.html").read_text(encoding="utf-8")
    monkeypatch.setattr(
        module,
        "fetch_html_resilient",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(requests.HTTPError("403")),
    )
    monkeypatch.setattr(module, "fetch_html_by_selenium", lambda *_args, **_kwargs: portal_html)
    monkeypatch.setattr(module, "get_cached_week_range", lambda: WEEK_RANGE)

    results = module.scrape_moenv_this_week()

    assert [item["title"] for item in results] == ["環境部備援新聞"]


def test_mjac_limited_official_fallback_parses_dated_titles(monkeypatch):
    module = import_module("news_scraper.scrapers.ministry.justice.mjac")
    fallback_html = (FIXTURE_ROOT / "mjac_latest.html").read_text(encoding="utf-8")
    monkeypatch.setattr(
        module,
        "fetch_html_resilient",
        lambda url, **_kwargs: fallback_html if "654264" in url else "<html></html>",
    )
    monkeypatch.setattr(module, "get_cached_week_range", lambda: WEEK_RANGE)

    results = module.scrape_mjac_this_week()

    assert [item["title"] for item in results] == ["115年7月29日矯正署備援新聞"]


def test_sfaa_reduced_coverage_mohw_fallback_filters_source(monkeypatch):
    module = import_module("news_scraper.scrapers.ministry.health.sfaa")
    list_html = (FIXTURE_ROOT / "mohw_list.html").read_text(encoding="utf-8")
    detail_html = (FIXTURE_ROOT / "mohw_detail.html").read_text(encoding="utf-8")

    def fetch(url, **_kwargs):
        if "mohw.gov.tw/lp-" in url:
            return list_html
        if "cp-16-1.html" in url:
            return detail_html
        return "<html></html>"

    monkeypatch.setattr(module, "fetch_html_resilient", fetch)
    monkeypatch.setattr(module, "get_cached_week_range", lambda: WEEK_RANGE)

    results = module.scrape_sfaa_this_week()

    assert [item["title"] for item in results] == ["社家署備援新聞"]
