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


def check_bundled_runtime() -> int:
    """Verify all modules needed by the default packaged run are importable."""

    from news_scraper.runtime import validate_runtime_environment

    validate_runtime_environment()
    print("封裝執行環境檢查通過。")
    return 0


if __name__ == "__main__":
    configure_utf8_stdio()
    if "--check-runtime" in sys.argv[1:]:
        raise SystemExit(check_bundled_runtime())
    raise SystemExit(main())
