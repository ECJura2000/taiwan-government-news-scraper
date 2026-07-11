"""Stable top-level entry point used by PyInstaller builds."""

import sys

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
            # Some embedded or replaced streams do not permit reconfiguration.
            continue


if __name__ == "__main__":
    configure_utf8_stdio()
    raise SystemExit(main())
