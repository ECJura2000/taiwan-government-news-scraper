"""Python command bridge for the Rust-native application.

This file does not import or execute the legacy Python scraper. It only locates
the Rust CLI and forwards subcommands such as ``collect`` and ``list-sources``.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path


def find_rust_cli() -> str:
    configured = os.environ.get("NEWS_SCRAPER_RUST_BIN")
    if configured:
        return configured
    project_root = Path(__file__).resolve().parents[1]
    candidates = [
        project_root / "target" / "debug" / "news-scraper",
        project_root / "target" / "debug" / "news-scraper.exe",
        project_root / "target" / "release" / "news-scraper",
        project_root / "target" / "release" / "news-scraper.exe",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    discovered = shutil.which("news-scraper")
    if discovered:
        return discovered
    raise SystemExit("找不到 Rust news-scraper；請先 cargo build --release，或設定 NEWS_SCRAPER_RUST_BIN")


def main() -> int:
    if len(sys.argv) == 1:
        print("用法：python scripts/python_compat.py collect|list-sources [參數]", file=sys.stderr)
        return 2
    return subprocess.call([find_rust_cli(), *sys.argv[1:]])


if __name__ == "__main__":
    raise SystemExit(main())
