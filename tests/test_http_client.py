import subprocess

import pytest
import requests

from news_scraper.config import RETRY_BACKOFF_FACTOR, RETRY_TOTAL
from news_scraper.errors import CurlRequestError, ParseError
from news_scraper.http import async_client
from news_scraper.http.client import (
    apply_response_encoding,
    fetch_html,
    fetch_html_by_curl,
    fetch_html_by_curl_with_headers,
    fetch_html_plain_insecure,
    fetch_html_resilient,
    fetch_json_data,
    get_effective_timeout,
    get_thread_session,
    merge_headers,
    send_request,
    set_retry_timeout_extra_seconds,
)
from news_scraper.monitoring import RunContext, classify_error, use_run_context


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


def test_insecure_fetch_rejects_non_allowlisted_host():
    with pytest.raises(requests.exceptions.SSLError, match="非白名單"):
        fetch_html_plain_insecure("https://example.com/news")


def test_insecure_fetch_records_allowlisted_host(monkeypatch):
    class FakeResponse:
        status_code = 200
        headers = {"Content-Type": "text/html; charset=utf-8"}
        text = "<html></html>"
        apparent_encoding = "utf-8"
        encoding = "utf-8"

        def raise_for_status(self):
            return None

    context = RunContext()
    monkeypatch.setattr("news_scraper.http.client.send_request", lambda *args, **kwargs: FakeResponse())

    with use_run_context(context):
        fetch_html_plain_insecure("https://www.moi.gov.tw/news")

    assert context.insecure_ssl_hosts == {"www.moi.gov.tw"}


def test_insecure_fetch_rejects_redirect_to_non_allowlisted_host(monkeypatch):
    class RedirectResponse:
        status_code = 302
        headers = {"Location": "https://example.com/redirected"}

    calls = []
    monkeypatch.setattr(
        "news_scraper.http.client.send_request",
        lambda *args, **kwargs: calls.append(args[2]) or RedirectResponse(),
    )

    with pytest.raises(requests.exceptions.SSLError, match="非白名單"):
        fetch_html_plain_insecure("https://www.moi.gov.tw/news")

    assert calls == ["https://www.moi.gov.tw/news"]


def test_insecure_fetch_allows_redirect_between_allowlisted_hosts(monkeypatch):
    class FakeResponse:
        def __init__(self, status_code, headers=None, text=""):
            self.status_code = status_code
            self.headers = headers or {"Content-Type": "text/html; charset=utf-8"}
            self.text = text
            self.apparent_encoding = "utf-8"
            self.encoding = "utf-8"

        def raise_for_status(self):
            return None

    responses = iter(
        [
            FakeResponse(302, {"Location": "https://www.dgpa.gov.tw/final"}),
            FakeResponse(200, text="<rss />"),
        ]
    )
    calls = []
    monkeypatch.setattr(
        "news_scraper.http.client.send_request",
        lambda *args, **kwargs: calls.append((args[2], kwargs["allow_redirects"])) or next(responses),
    )

    assert fetch_html_plain_insecure("https://www.moi.gov.tw/news") == "<rss />"
    assert calls == [
        ("https://www.moi.gov.tw/news", False),
        ("https://www.dgpa.gov.tw/final", False),
    ]


def test_fetch_html_uses_secure_curl_before_insecure_fallback(monkeypatch):
    calls = []
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(requests.exceptions.SSLError("bad certificate")),
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: calls.append(("curl", kwargs["insecure"])) or "<rss />",
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_plain_insecure",
        lambda *args, **kwargs: calls.append(("insecure", True)) or "<rss />",
    )

    assert fetch_html("https://www.dgpa.gov.tw/rsscon?uid=82") == "<rss />"
    assert calls == [("curl", False)]


def test_fetch_html_only_uses_insecure_after_secure_curl_fails(monkeypatch):
    calls = []
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(requests.exceptions.SSLError("bad certificate")),
    )

    def fail_secure_curl(*args, **kwargs):
        calls.append(("curl", kwargs["insecure"]))
        raise requests.RequestException("curl failed")

    monkeypatch.setattr("news_scraper.http.client.fetch_html_by_curl_with_headers", fail_secure_curl)
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_plain_insecure",
        lambda *args, **kwargs: calls.append(("insecure", True)) or "<rss />",
    )

    assert fetch_html("https://www.dgpa.gov.tw/rsscon?uid=82") == "<rss />"
    assert calls == [("curl", False), ("insecure", True)]


def test_resilient_fetch_also_prefers_secure_curl_before_insecure(monkeypatch):
    calls = []

    def fail_requests(*args, **kwargs):
        calls.append("requests")
        raise requests.RequestException("requests failed")

    monkeypatch.setattr("news_scraper.http.client.fetch_html", fail_requests)
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: calls.append("secure_curl") or "<html />",
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_plain_insecure",
        lambda *args, **kwargs: calls.append("insecure") or "<html />",
    )

    assert fetch_html_resilient("https://www.dgpa.gov.tw/rsscon?uid=82") == "<html />"
    assert calls == ["requests", "secure_curl"]


def test_resilient_fetch_continues_after_structured_curl_error(monkeypatch):
    calls = []

    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            CurlRequestError(
                "timed out",
                url="https://www.dgpa.gov.tw/rsscon?uid=82",
                exit_code=28,
                error_category="timeout",
            )
        ),
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: calls.append("secure_curl") or "<html />",
    )

    assert fetch_html_resilient("https://www.dgpa.gov.tw/rsscon?uid=82") == "<html />"
    assert calls == ["secure_curl"]


def test_curl_timeout_has_structured_category(monkeypatch):
    class Completed:
        returncode = 28
        stdout = "\n__NEWS_SCRAPER_HTTP_STATUS__:000"
        stderr = "curl: (28) Operation timed out after 5000 milliseconds"

    monkeypatch.setattr("news_scraper.http.client.subprocess.run", lambda *args, **kwargs: Completed())

    with pytest.raises(CurlRequestError) as exc_info:
        fetch_html_by_curl("https://example.test/news", timeout=5)

    assert exc_info.value.exit_code == 28
    assert exc_info.value.error_category == "timeout"
    assert classify_error(exc_info.value) == "timeout"
    assert "Total" not in str(exc_info.value)


def test_curl_http_error_preserves_status(monkeypatch):
    class Completed:
        returncode = 22
        stdout = "<html>forbidden</html>\n__NEWS_SCRAPER_HTTP_STATUS__:403"
        stderr = "curl: (22) The requested URL returned error: 403"

    monkeypatch.setattr("news_scraper.http.client.subprocess.run", lambda *args, **kwargs: Completed())

    with pytest.raises(CurlRequestError) as exc_info:
        fetch_html_by_curl("https://example.test/news", timeout=5)

    assert exc_info.value.http_status == 403
    assert classify_error(exc_info.value) == "http"


def test_curl_subprocess_timeout_has_structured_category(monkeypatch):
    def expire(*args, **kwargs):
        raise subprocess.TimeoutExpired(cmd="curl", timeout=7)

    monkeypatch.setattr("news_scraper.http.client.subprocess.run", expire)

    with pytest.raises(CurlRequestError) as exc_info:
        fetch_html_by_curl("https://example.test/news", timeout=5)

    assert exc_info.value.error_category == "timeout"
    assert exc_info.value.exit_code is None


def test_curl_success_returns_body_without_status_marker(monkeypatch):
    class Completed:
        returncode = 0
        stdout = "<html>ok</html>\n__NEWS_SCRAPER_HTTP_STATUS__:200"
        stderr = ""

    monkeypatch.setattr("news_scraper.http.client.subprocess.run", lambda *args, **kwargs: Completed())

    assert fetch_html_by_curl("https://example.test/news", timeout=None) == "<html>ok</html>"


def test_curl_replaces_isolated_invalid_utf8_bytes(monkeypatch):
    captured = {}

    class Completed:
        returncode = 0
        stdout = "<html>ok\ufffd</html>\n__NEWS_SCRAPER_HTTP_STATUS__:200"
        stderr = ""

    def fake_run(*args, **kwargs):
        captured.update(kwargs)
        return Completed()

    monkeypatch.setattr("news_scraper.http.client.subprocess.run", fake_run)

    assert fetch_html_by_curl("https://example.test/news", timeout=5) == "<html>ok\ufffd</html>"
    assert captured["encoding"] == "utf-8"
    assert captured["errors"] == "replace"


def test_curl_with_headers_builds_post_request(monkeypatch):
    captured = {}

    def fake_run(command, *, url, timeout):
        captured["command"] = command
        captured["url"] = url
        captured["timeout"] = timeout
        return "ok"

    monkeypatch.setattr("news_scraper.http.client._run_curl", fake_run)

    assert (
        fetch_html_by_curl_with_headers(
            "https://example.test/news",
            timeout=9,
            extra_headers={"X-Test": "yes"},
            method="POST",
            data="payload",
        )
        == "ok"
    )
    assert ["-X", "POST"] == captured["command"][captured["command"].index("-X") :][:2]
    assert "X-Test: yes" in captured["command"]
    assert "payload" in captured["command"]


def test_curl_insecure_mode_rejects_non_allowlisted_host():
    with pytest.raises(requests.exceptions.SSLError, match="非白名單"):
        fetch_html_by_curl_with_headers("https://example.test/news", insecure=True)


def test_response_helpers_apply_headers_and_encoding():
    class FakeSession:
        headers = {"User-Agent": "agent"}

        def request(self, *args, **kwargs):
            return args, kwargs

    class FakeResponse:
        headers = {"Content-Type": "text/html"}
        apparent_encoding = "big5"
        encoding = None

    session = FakeSession()
    assert merge_headers(session, {"X-Test": "yes"}) == {"User-Agent": "agent", "X-Test": "yes"}
    assert merge_headers(session) is None
    args, kwargs = send_request(
        session,
        "post",
        "https://example.test",
        5,
        data="body",
        extra_headers={"X-Test": "yes"},
        allow_redirects=False,
    )
    assert args == ("POST", "https://example.test")
    assert kwargs["headers"]["X-Test"] == "yes"
    assert kwargs["allow_redirects"] is False

    response = FakeResponse()
    apply_response_encoding(response)
    assert response.encoding == "big5"


def test_retry_timeout_extra_is_applied_and_clamped():
    context = RunContext()
    with use_run_context(context):
        set_retry_timeout_extra_seconds(7)
        assert get_effective_timeout(5) == 12
        set_retry_timeout_extra_seconds(-5)
        assert get_effective_timeout(5) == 5
        assert get_effective_timeout(None) is None


def test_json_parse_failure_is_classified(monkeypatch):
    monkeypatch.setattr("news_scraper.http.client.fetch_html", lambda *args, **kwargs: "not-json")

    with pytest.raises(ParseError, match="JSON 解析失敗"):
        fetch_json_data("https://example.test/data")


def test_insecure_redirect_without_location_fails(monkeypatch):
    class RedirectResponse:
        status_code = 302
        headers = {}

        def raise_for_status(self):
            return None

    monkeypatch.setattr("news_scraper.http.client.send_request", lambda *args, **kwargs: RedirectResponse())

    with pytest.raises(requests.RequestException, match="缺少 Location"):
        fetch_html_plain_insecure("https://www.moi.gov.tw/news")


def test_non_allowlisted_ssl_failure_preserves_original_error(monkeypatch):
    original = requests.exceptions.SSLError("bad certificate")
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_response",
        lambda *args, **kwargs: (_ for _ in ()).throw(original),
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: (_ for _ in ()).throw(
            CurlRequestError(
                "curl ssl failed",
                url="https://example.test/news",
                exit_code=60,
                error_category="ssl",
            )
        ),
    )

    with pytest.raises(requests.exceptions.SSLError) as exc_info:
        fetch_html("https://example.test/news")

    assert exc_info.value is original


def test_resilient_fetch_raises_last_error(monkeypatch):
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html",
        lambda *args, **kwargs: (_ for _ in ()).throw(requests.Timeout("requests timeout")),
    )
    last_error = CurlRequestError(
        "curl timeout",
        url="https://example.test/news",
        exit_code=28,
        error_category="timeout",
    )
    monkeypatch.setattr(
        "news_scraper.http.client.fetch_html_by_curl_with_headers",
        lambda *args, **kwargs: (_ for _ in ()).throw(last_error),
    )

    with pytest.raises(CurlRequestError) as exc_info:
        fetch_html_resilient("https://example.test/news")

    assert exc_info.value is last_error


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
