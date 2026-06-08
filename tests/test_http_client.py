import requests

from news_scraper.config import RETRY_BACKOFF_FACTOR, RETRY_TOTAL
from news_scraper.http import async_client
from news_scraper.http.client import get_thread_session


def test_thread_session_uses_configured_retries():
    session = get_thread_session()
    adapter = session.get_adapter("https://")
    retry = adapter.max_retries

    assert retry.total == RETRY_TOTAL
    assert retry.connect == RETRY_TOTAL
    assert retry.read == RETRY_TOTAL
    assert retry.status == RETRY_TOTAL
    assert retry.backoff_factor == RETRY_BACKOFF_FACTOR
    assert 429 in retry.status_forcelist
    assert 503 in retry.status_forcelist


def test_fetch_paginated_soups_threaded_honors_max_workers(monkeypatch):
    seen_worker_counts = []

    class FakeExecutor:
        def __init__(self, max_workers):
            seen_worker_counts.append(max_workers)
            self.max_workers = max_workers

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, traceback):
            return False

        def submit(self, fn, page_url_pair):
            class FakeFuture:
                def result(self):
                    return fn(page_url_pair)

            return FakeFuture()

    monkeypatch.setattr(async_client, "ThreadPoolExecutor", FakeExecutor)
    monkeypatch.setattr(async_client, "as_completed", lambda futures: list(futures))
    monkeypatch.setattr(async_client, "fetch_html", lambda url, timeout, extra_headers: "<html></html>")

    async_client.fetch_paginated_soups_threaded(
        [(1, "https://example.com/1"), (2, "https://example.com/2"), (3, "https://example.com/3")],
        max_workers=2,
    )

    assert seen_worker_counts == [2]


def test_fetch_paginated_soups_falls_back_with_requested_worker_count(monkeypatch):
    seen_worker_counts = []

    async def fake_async_fetch_paginated_soups(*args, **kwargs):
        raise requests.RequestException("network failed")

    def fake_threaded(page_url_pairs, max_workers, timeout, extra_headers):
        seen_worker_counts.append(max_workers)
        return {}

    monkeypatch.setattr(async_client, "AIOHTTP_IMPORT_ERROR", None)
    monkeypatch.setattr(async_client, "async_fetch_paginated_soups", fake_async_fetch_paginated_soups)
    monkeypatch.setattr(async_client, "fetch_paginated_soups_threaded", fake_threaded)

    async_client.fetch_paginated_soups([(1, "https://example.com/1")], max_workers=3)

    assert seen_worker_counts == [3]
