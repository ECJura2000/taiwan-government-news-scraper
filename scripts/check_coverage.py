from __future__ import annotations

import argparse
import json
from pathlib import Path

CORE_MINIMUM = 60.0
CORE_EXCLUDED_FILES = {"news_scraper/gui.py"}
CORE_EXCLUDED_PREFIXES = ("news_scraper/scrapers/ministry/",)
CRITICAL_FILE_MINIMUMS = {
    "news_scraper/http/client.py": 80.0,
    "news_scraper/main.py": 80.0,
    "news_scraper/monitoring.py": 80.0,
    "news_scraper/scrapers/ministry/veterans/vghtpe.py": 80.0,
}


def is_core_file(path: str) -> bool:
    return path not in CORE_EXCLUDED_FILES and not path.startswith(CORE_EXCLUDED_PREFIXES)


def calculate_core_coverage(files: dict[str, dict]) -> float:
    summaries = [
        value["summary"]
        for path, value in files.items()
        if path.startswith("news_scraper/") and is_core_file(path)
    ]
    statements = sum(int(summary["num_statements"]) for summary in summaries)
    covered = sum(int(summary["covered_lines"]) for summary in summaries)
    if statements == 0:
        raise ValueError("coverage report 沒有核心套件 statements")
    return covered * 100.0 / statements


def validate_coverage(report: dict) -> list[str]:
    files = report.get("files")
    if not isinstance(files, dict):
        return ["coverage report 缺少 files mapping"]

    failures = []
    try:
        core_coverage = calculate_core_coverage(files)
    except (KeyError, TypeError, ValueError) as exc:
        failures.append(str(exc))
    else:
        if core_coverage < CORE_MINIMUM:
            failures.append("核心覆蓋率 {:.2f}% 低於 {:.2f}%".format(core_coverage, CORE_MINIMUM))

    for path, minimum in CRITICAL_FILE_MINIMUMS.items():
        file_report = files.get(path)
        if not isinstance(file_report, dict):
            failures.append("coverage report 缺少關鍵檔案：{}".format(path))
            continue
        try:
            coverage = float(file_report["summary"]["percent_covered"])
        except (KeyError, TypeError, ValueError):
            failures.append("coverage report 無法讀取關鍵檔案覆蓋率：{}".format(path))
            continue
        if coverage < minimum:
            failures.append("{} 覆蓋率 {:.2f}% 低於 {:.2f}%".format(path, coverage, minimum))
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate package and critical-file coverage gates.")
    parser.add_argument("report", type=Path)
    args = parser.parse_args(argv)
    report = json.loads(args.report.read_text(encoding="utf-8"))
    failures = validate_coverage(report)
    if failures:
        raise SystemExit("\n".join(failures))

    core_coverage = calculate_core_coverage(report["files"])
    print("核心覆蓋率 {:.2f}%，關鍵模組均達門檻。".format(core_coverage))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
