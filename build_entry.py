"""Stable top-level entry point used by PyInstaller builds."""

import sys

from news_scraper import bundled_scrapers  # noqa: F401
from news_scraper.main import main


def configure_utf8_stdio() -> None:
    """Prevent Windows frozen apps from failing on redirected Chinese output."""

    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if not callable(reconfigure):
            continue
        try:
            reconfigure(encoding="utf-8", errors="backslashreplace")
        except (OSError, ValueError):
            continue


def check_bundled_runtime() -> int:
    """Verify third-party and dynamically loaded scraper modules are importable."""

    from news_scraper.runtime import validate_runtime_environment
    from news_scraper.scrapers.registry import SCRAPER_REGISTRY

    validate_runtime_environment()
    for source_name in SCRAPER_REGISTRY:
        SCRAPER_REGISTRY[source_name]
    print("封裝執行環境與全部爬蟲模組檢查通過。")
    return 0


if __name__ == "__main__":
    configure_utf8_stdio()
    if "--check-runtime" in sys.argv[1:]:
        raise SystemExit(check_bundled_runtime())
    raise SystemExit(main())
