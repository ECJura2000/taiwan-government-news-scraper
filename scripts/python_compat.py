"""Optional legacy Python command bridge during the Rust migration.

Normal CLI and Tauri execution must not import or spawn this module. It exists
only for operators who need to run the legacy Python implementation explicitly
while native Rust parity is being completed.
"""

from __future__ import annotations

import subprocess
import sys


def main() -> int:
    if len(sys.argv) == 1:
        print("用法：python scripts/python_compat.py <news_scraper 參數>", file=sys.stderr)
        return 2
    return subprocess.call([sys.executable, "-m", "news_scraper", *sys.argv[1:]])


if __name__ == "__main__":
    raise SystemExit(main())
