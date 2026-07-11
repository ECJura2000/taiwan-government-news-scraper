"""Stable top-level entry point used by PyInstaller builds."""

from news_scraper.main import main


if __name__ == "__main__":
    raise SystemExit(main())
