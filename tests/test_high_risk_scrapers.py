from datetime import date
from importlib import import_module
from pathlib import Path

import pytest

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

    with pytest.raises(ValueError, match="找不到"):
        getattr(module, function_name)()
