from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from datetime import date
from pathlib import Path

from news_scraper.scrapers.registry import SCRAPER_REGISTRY

HIGH_RISK_SOURCES = ("榮總", "司法院", "財政部")
DEFAULT_BATCH_COUNT = 7


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run rotating live parser smoke checks.")
    parser.add_argument("--batch-count", type=int, default=DEFAULT_BATCH_COUNT)
    parser.add_argument("--batch-index", type=int)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--sources", nargs="+")
    return parser.parse_args(argv)


def select_sources(
    all_sources: list[str],
    *,
    batch_count: int,
    batch_index: int,
    high_risk_sources: tuple[str, ...] = HIGH_RISK_SOURCES,
) -> list[str]:
    if batch_count < 1:
        raise ValueError("batch_count 必須大於 0")
    normalized_index = batch_index % batch_count
    rotating = [source for index, source in enumerate(all_sources) if index % batch_count == normalized_index]
    return list(dict.fromkeys([*high_risk_sources, *rotating]))


def parse_json_summary(stdout: str) -> dict:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "status" in value:
            return value
    raise ValueError("找不到 news_scraper JSON summary")


def run_source(source: str, timeout: int) -> tuple[bool, str]:
    with tempfile.TemporaryDirectory(prefix="news-scraper-smoke-") as temp_dir:
        root = Path(temp_dir)
        command = [
            sys.executable,
            "-m",
            "news_scraper",
            "--sources",
            source,
            "--output-dir",
            str(root / "output"),
            "--report-dir",
            str(root / "reports"),
            "--max-workers",
            "1",
            "--json-summary",
            "--fail-on-source-error",
        ]
        try:
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return False, "{}: smoke timeout after {} seconds".format(source, timeout)

    try:
        summary = parse_json_summary(completed.stdout)
    except ValueError as exc:
        detail = (completed.stderr or completed.stdout)[-1000:].strip()
        return False, "{}: {} ({})".format(source, exc, detail)

    failed_sources = summary.get("failed_sources") or []
    if completed.returncode != 0 or failed_sources:
        return False, "{}: status={} failed_sources={} exit_code={}".format(
            source,
            summary.get("status"),
            failed_sources,
            completed.returncode,
        )
    return True, "{}: status={} news_count={}".format(source, summary.get("status"), summary.get("news_count"))


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    available_sources = list(SCRAPER_REGISTRY)
    if args.sources:
        unknown = [source for source in args.sources if source not in SCRAPER_REGISTRY]
        if unknown:
            raise ValueError("不支援的 smoke 來源：{}".format("、".join(unknown)))
        selected_sources = list(dict.fromkeys(args.sources))
    else:
        batch_index = args.batch_index if args.batch_index is not None else date.today().toordinal()
        selected_sources = select_sources(
            available_sources,
            batch_count=args.batch_count,
            batch_index=batch_index,
        )

    failures = []
    print("Smoke sources: {}".format("、".join(selected_sources)))
    for source in selected_sources:
        success, message = run_source(source, timeout=args.timeout)
        print(message)
        if not success:
            failures.append(source)

    if failures:
        print("::error::Parser smoke failed: {}".format("、".join(failures)))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
