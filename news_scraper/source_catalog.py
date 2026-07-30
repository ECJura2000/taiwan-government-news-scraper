from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Callable, TypeVar
from urllib.parse import urlparse

from .config import ORDERED_SOURCE_NAMES, SCRAPE_DIFFICULTY_ORDER, URLS, get_source_urls

T = TypeVar("T")


@dataclass(frozen=True)
class SourceRoute:
    id: str
    url: str
    kind: str = "html"
    parser: str = "primary"
    priority: int = 1
    official: bool = True
    coverage_reduced: bool = False


SOURCE_ROUTE_OVERRIDES: dict[str, tuple[SourceRoute, ...]] = {
    "漁業署": (
        SourceRoute("primary-html", URLS["漁業署"], "html", "fa-html", 1),
        SourceRoute("official-rss", "https://www.fa.gov.tw/wm_DATA.php?data=news", "rss", "standard-rss", 2),
    ),
    "農業金融署": (
        SourceRoute("primary-html", URLS["農業金融署"], "html", "afna-html", 1),
        SourceRoute("official-rss", "https://www.afna.gov.tw/wm_DATA.php?data=news", "rss", "standard-rss", 2),
    ),
    "工程會": (
        SourceRoute("primary-html", URLS["工程會"], "html", "pcc-html", 1),
        SourceRoute(
            "official-open-data",
            "https://www.pcc.gov.tw/content/opendata?item=news&n=CF5DF99964D9DB8E",
            "rss",
            "standard-rss",
            2,
        ),
    ),
    "環境部": (
        SourceRoute("primary-html", URLS["環境部"], "html", "moenv-html", 1),
        SourceRoute(
            "news-portal",
            "https://enews.moenv.gov.tw/",
            "browser",
            "moenv-news-portal-browser",
            2,
        ),
    ),
    "矯正署": (
        SourceRoute("primary-html", URLS["矯正署"], "html", "mjac-html", 1),
        SourceRoute(
            "official-latest-news",
            "https://www.mjac.moj.gov.tw/4786/654264/",
            "html",
            "mjac-html",
            2,
            coverage_reduced=True,
        ),
    ),
    "社家署": (
        SourceRoute("primary-html", URLS["社家署"], "html", "sfaa-html", 1),
        SourceRoute(
            "mohw-source-mirror",
            "https://www.mohw.gov.tw/lp-16-1-40.html",
            "html",
            "mohw-source-filter",
            2,
            coverage_reduced=True,
        ),
    ),
}


@dataclass(frozen=True)
class SourceSpec:
    name: str
    url: str
    urls: tuple[str, ...]
    routes: tuple[SourceRoute, ...]
    module: str
    function: str
    order: int
    difficulty: int


def get_source_routes(source: str) -> tuple[SourceRoute, ...]:
    configured = SOURCE_ROUTE_OVERRIDES.get(source)
    if configured is not None:
        return configured
    return tuple(
        SourceRoute(
            id="primary" if index == 1 else "alternate-{}".format(index),
            url=url,
            priority=index,
        )
        for index, url in enumerate(get_source_urls(source), 1)
    )


def _annotate_route_error(error: BaseException, route: SourceRoute) -> None:
    for name, value in (
        ("route_id", route.id),
        ("url", route.url),
        ("url_host", urlparse(route.url).netloc.lower()),
    ):
        if not getattr(error, name, None):
            try:
                setattr(error, name, value)
            except (AttributeError, TypeError):
                pass


def execute_source_routes(
    source: str,
    routes: tuple[SourceRoute, ...],
    handler: Callable[[SourceRoute], T],
) -> T:
    """Try official routes in priority order and retain route-level evidence."""

    from .monitoring import get_current_run_context

    if not routes:
        raise ValueError("{} 沒有設定來源入口。".format(source))
    context = get_current_run_context()
    last_error: BaseException | None = None
    for route in sorted(routes, key=lambda item: item.priority):
        started = time.perf_counter()
        try:
            result = handler(route)
        except Exception as exc:
            _annotate_route_error(exc, route)
            if context is not None:
                context.record_route_attempt(
                    source,
                    route,
                    elapsed_seconds=time.perf_counter() - started,
                    error=exc,
                )
            last_error = exc
            continue
        if context is not None:
            context.record_route_attempt(
                source,
                route,
                elapsed_seconds=time.perf_counter() - started,
                item_count=len(result) if hasattr(result, "__len__") else 0,
            )
            context.record_final_route(source, route)
        return result
    assert last_error is not None
    raise last_error


def build_source_catalog(scraper_specs: dict[str, tuple[str, str]]) -> dict[str, SourceSpec]:
    expected = set(ORDERED_SOURCE_NAMES)
    configured = {
        "URLS": set(URLS),
        "SCRAPE_DIFFICULTY_ORDER": set(SCRAPE_DIFFICULTY_ORDER),
        "SCRAPER_SPECS": set(scraper_specs),
    }
    mismatches = {name: sorted(keys ^ expected) for name, keys in configured.items() if keys != expected}
    if mismatches:
        raise RuntimeError(f"來源設定不一致：{mismatches}")
    return {
        name: SourceSpec(
            name=name,
            url=URLS[name],
            urls=get_source_urls(name),
            routes=get_source_routes(name),
            module=scraper_specs[name][0],
            function=scraper_specs[name][1],
            order=position,
            difficulty=SCRAPE_DIFFICULTY_ORDER[name],
        )
        for position, name in enumerate(ORDERED_SOURCE_NAMES, 1)
    }
