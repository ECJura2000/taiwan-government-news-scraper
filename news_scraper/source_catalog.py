from __future__ import annotations

from dataclasses import dataclass

from .config import ORDERED_SOURCE_NAMES, SCRAPE_DIFFICULTY_ORDER, URLS


@dataclass(frozen=True)
class SourceSpec:
    name: str
    url: str | list[str]
    module: str
    function: str
    order: int
    difficulty: int


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
            module=scraper_specs[name][0],
            function=scraper_specs[name][1],
            order=position,
            difficulty=SCRAPE_DIFFICULTY_ORDER[name],
        )
        for position, name in enumerate(ORDERED_SOURCE_NAMES, 1)
    }
