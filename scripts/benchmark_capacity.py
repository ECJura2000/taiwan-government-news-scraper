from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import platform
from pathlib import Path
import statistics
import sys
import time
import tracemalloc

PROJECT_ROOT = Path(__file__).resolve().parents[1]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from news_scraper.models import make_news_item  # noqa: E402
from news_scraper.quality import process_news_quality  # noqa: E402


def percentile(samples: list[float], p: float) -> float:
    ordered = sorted(samples)
    return ordered[min(len(ordered) - 1, round((len(ordered) - 1) * p))]


def measure(operation, iterations: int = 5) -> dict:
    operation()
    samples = []
    peaks = []
    for _ in range(iterations):
        tracemalloc.start()
        started = time.perf_counter()
        operation()
        samples.append((time.perf_counter() - started) * 1000)
        _, peak = tracemalloc.get_traced_memory()
        tracemalloc.stop()
        peaks.append(peak / 1024 / 1024)
    return {
        "best_ms": round(min(samples), 3),
        "mean_ms": round(statistics.fmean(samples), 3),
        "p95_ms": round(percentile(samples, 0.95), 3),
        "peak_memory_mb": round(max(peaks), 3),
    }


def make_items(size: int):
    return [
        make_news_item("測試來源", "測試來源", "2026-06-13", f"新聞 {index // 2}", f"https://example.com/{index // 2}")
        for index in range(size)
    ]


def concurrency_measure(workers: int, jobs: int = 64) -> dict:
    def task(index):
        time.sleep(0.002)
        return index

    return measure(lambda: list(ThreadPoolExecutor(max_workers=workers).map(task, range(jobs))), iterations=3)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sizes", nargs="+", type=int, default=[1000, 10000, 100000])
    parser.add_argument("--workers", nargs="+", type=int, default=[1, 4, 16, 30])
    parser.add_argument("--output")
    args = parser.parse_args()
    result = {
        "environment": {
            "python": platform.python_version(),
            "platform": platform.platform(),
            "processor": platform.processor(),
        },
        "dedupe_quality": {
            str(size): measure(lambda items=make_items(size): process_news_quality(items, ["測試來源"]))
            for size in args.sizes
        },
        "concurrency": {str(workers): concurrency_measure(workers) for workers in args.workers},
    }
    payload = json.dumps(result, ensure_ascii=False, indent=2)
    print(payload)
    if args.output:
        Path(args.output).write_text(payload + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
