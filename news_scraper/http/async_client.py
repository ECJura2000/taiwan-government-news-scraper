import asyncio
import logging
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Any, Mapping

from bs4 import BeautifulSoup

from ..config import (
    ASYNC_MAX_CONNECTIONS,
    ASYNC_MAX_PER_HOST,
    ASYNC_PAGE_BATCH_WORKERS,
    ASYNC_PAGE_TIMEOUT,
    HEADERS,
    PARSER,
)
from .client import (
    fetch_html,
    get_effective_timeout,
    remember_ssl_verify_failure,
    should_fallback_ssl,
    should_skip_ssl_verify,
)

logger = logging.getLogger(__name__)

try:
    import aiohttp
except ImportError as exc:
    aiohttp = None
    AIOHTTP_IMPORT_ERROR = exc
else:
    AIOHTTP_IMPORT_ERROR = None


def _require_aiohttp():
    if aiohttp is None:
        raise ImportError("aiohttp 未安裝，無法使用非同步 HTTP 抓取")
    return aiohttp


async def async_fetch_page_text(session: Any, page: int, url: str, timeout: int | float = ASYNC_PAGE_TIMEOUT) -> tuple[int, str]:
    async with session.get(url, timeout=timeout) as response:
        response.raise_for_status()
        return page, await response.text()


async def async_fetch_page_text_with_fallback(
    page: int,
    url: str,
    timeout: int | float = ASYNC_PAGE_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> tuple[int, str]:
    aiohttp_module = _require_aiohttp()
    timeout = get_effective_timeout(timeout)
    request_headers = dict(HEADERS)
    if extra_headers:
        request_headers.update(extra_headers)

    timeout_cfg = aiohttp_module.ClientTimeout(total=timeout)
    connector = aiohttp_module.TCPConnector(
        limit=ASYNC_MAX_CONNECTIONS,
        limit_per_host=ASYNC_MAX_PER_HOST,
        ssl=True,
    )
    try:
        async with aiohttp_module.ClientSession(headers=request_headers, connector=connector, timeout=timeout_cfg) as session:
            return await async_fetch_page_text(session, page, url, timeout=timeout)
    except aiohttp_module.ClientSSLError:
        if not should_fallback_ssl(url):
            raise

    insecure_connector = aiohttp_module.TCPConnector(
        limit=ASYNC_MAX_CONNECTIONS,
        limit_per_host=ASYNC_MAX_PER_HOST,
        ssl=False,
    )
    async with aiohttp_module.ClientSession(headers=request_headers, connector=insecure_connector, timeout=timeout_cfg) as session:
        return await async_fetch_page_text(session, page, url, timeout=timeout)


async def async_fetch_paginated_soups(
    page_url_pairs: list[tuple[int, str]],
    timeout: int | float = ASYNC_PAGE_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
    max_connections: int = ASYNC_MAX_CONNECTIONS,
) -> dict[int, BeautifulSoup]:
    aiohttp_module = _require_aiohttp()
    timeout = get_effective_timeout(timeout)
    request_headers = dict(HEADERS)
    if extra_headers:
        request_headers.update(extra_headers)

    async def fetch_pairs(pairs: list[tuple[int, str]], verify_ssl: bool):
        if not pairs:
            return []
        connector = aiohttp_module.TCPConnector(
            limit=max(1, max_connections),
            limit_per_host=max(1, min(ASYNC_MAX_PER_HOST, max_connections)),
            ssl=verify_ssl,
        )
        timeout_cfg = aiohttp_module.ClientTimeout(total=timeout)
        async with aiohttp_module.ClientSession(headers=request_headers, connector=connector, timeout=timeout_cfg) as session:
            tasks = [async_fetch_page_text(session, page, url, timeout=timeout) for page, url in pairs]
            return await asyncio.gather(*tasks, return_exceptions=True)

    collected: dict[int, BeautifulSoup] = {}
    errors: list[Exception] = []
    ssl_retry_pairs: list[tuple[int, str]] = []
    secure_pairs: list[tuple[int, str]] = []
    insecure_pairs: list[tuple[int, str]] = []
    for page, url in page_url_pairs:
        if should_skip_ssl_verify(url):
            insecure_pairs.append((page, url))
        else:
            secure_pairs.append((page, url))

    secure_results = await fetch_pairs(secure_pairs, verify_ssl=True)
    for (page, url), result in zip(secure_pairs, secure_results):
        if isinstance(result, Exception):
            if isinstance(result, aiohttp_module.ClientSSLError) and should_fallback_ssl(url):
                remember_ssl_verify_failure(url)
                ssl_retry_pairs.append((page, url))
            else:
                errors.append(result)
            continue
        result_page, html = result
        collected[result_page] = BeautifulSoup(html, PARSER)

    insecure_results = await fetch_pairs(insecure_pairs + ssl_retry_pairs, verify_ssl=False)
    for (page, _), result in zip(insecure_pairs + ssl_retry_pairs, insecure_results):
        if isinstance(result, Exception):
            errors.append(result)
            continue
        result_page, html = result
        collected[result_page] = BeautifulSoup(html, PARSER)

    if errors:
        raise errors[0]
    return collected


def fetch_paginated_soups(
    page_url_pairs: list[tuple[int, str]],
    max_workers: int = ASYNC_PAGE_BATCH_WORKERS,
    timeout: int | float = ASYNC_PAGE_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> dict[int, BeautifulSoup]:
    if AIOHTTP_IMPORT_ERROR is not None:
        return fetch_paginated_soups_threaded(
            page_url_pairs,
            max_workers=max_workers,
            timeout=timeout,
            extra_headers=extra_headers,
        )

    try:
        page_soups = asyncio.run(
            async_fetch_paginated_soups(
                page_url_pairs,
                timeout=timeout,
                extra_headers=extra_headers,
                max_connections=max_workers,
            )
        )
        missing_page_url_pairs = [
            page_url_pair
            for page_url_pair in page_url_pairs
            if page_url_pair[0] not in page_soups
        ]
        if missing_page_url_pairs:
            logger.warning("aiohttp 多頁抓取缺少 %s 頁，僅補抓缺頁", len(missing_page_url_pairs))
            page_soups.update(
                fetch_paginated_soups_threaded(
                    missing_page_url_pairs,
                    max_workers=max_workers,
                    timeout=timeout,
                    extra_headers=extra_headers,
                )
            )
        return page_soups
    except Exception as exc:
        logger.warning("aiohttp 多頁抓取失敗，改用 threaded requests 備援：%s", exc)
        return fetch_paginated_soups_threaded(
            page_url_pairs,
            max_workers=max_workers,
            timeout=timeout,
            extra_headers=extra_headers,
        )


def fetch_paginated_soups_threaded(
    page_url_pairs: list[tuple[int, str]],
    max_workers: int = ASYNC_PAGE_BATCH_WORKERS,
    timeout: int | float = ASYNC_PAGE_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> dict[int, BeautifulSoup]:
    if not page_url_pairs:
        return {}

    def fetch_one(page_url_pair: tuple[int, str]) -> tuple[int, BeautifulSoup]:
        page, url = page_url_pair
        return page, BeautifulSoup(fetch_html(url, timeout=timeout, extra_headers=extra_headers), PARSER)

    results: dict[int, BeautifulSoup] = {}
    worker_count = max(1, min(max_workers, len(page_url_pairs)))
    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        future_map = {
            executor.submit(fetch_one, page_url_pair): page_url_pair
            for page_url_pair in page_url_pairs
        }
        for future in as_completed(future_map):
            result_page, soup = future.result()
            results[result_page] = soup
    return results
