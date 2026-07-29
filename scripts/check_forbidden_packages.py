from __future__ import annotations

import argparse
import json
from pathlib import Path


FORBIDDEN_PACKAGES = ("numpy", "pandas")


def find_forbidden_entries(report: dict) -> list[str]:
    found = set()
    for entry in report.get("entries", []):
        normalized = str(entry.get("name", "")).replace("\\", "/").lower()
        top_level = normalized.split("/", 1)[0].split(".", 1)[0]
        if top_level in FORBIDDEN_PACKAGES:
            found.add(top_level)
    return sorted(found)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Ensure removed packages are absent from a build.")
    parser.add_argument("build_report", type=Path)
    args = parser.parse_args(argv)
    report = json.loads(args.build_report.read_text(encoding="utf-8"))
    forbidden = find_forbidden_entries(report)
    if forbidden:
        raise SystemExit("封裝仍包含：{}".format("、".join(forbidden)))
    print("封裝未包含 pandas／numpy。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
