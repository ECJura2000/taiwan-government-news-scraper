import importlib
import requests

from news_scraper.errors import ParseError
from news_scraper.monitoring import RunContext
from news_scraper.scrapers import registry

main = importlib.import_module("news_scraper.main")


def test_download_failure_is_retried_but_parse_failure_is_not(monkeypatch):
    monkeypatch.setitem(registry.SCRAPER_REGISTRY._cache, "下載錯誤", lambda: (_ for _ in ()).throw(requests.Timeout("slow")))
    monkeypatch.setitem(registry.SCRAPER_REGISTRY._cache, "解析錯誤", lambda: (_ for _ in ()).throw(ParseError("bad payload")))
    monkeypatch.setitem(registry.SCRAPER_REGISTRY._source_specs, "下載錯誤", object())
    monkeypatch.setitem(registry.SCRAPER_REGISTRY._source_specs, "解析錯誤", object())
    context = RunContext()

    _, retryable = main.collect_news_for_sources_once(["下載錯誤", "解析錯誤"], 2, context=context)

    assert retryable == ["下載錯誤"]
    assert "解析錯誤" in context.failed_sources
