import heapq
from dataclasses import dataclass, field


@dataclass(order=True)
class SourceJob:
    priority: tuple[float, float, float, int]
    source: str = field(compare=False)
    reason: dict = field(compare=False, default_factory=dict)


def source_history(source, recent_reports):
    runs = []
    for report in recent_reports:
        attempts = [
            attempt
            for attempt in report.get("source_attempts", [])
            if attempt.get("source") == source
        ]
        if attempts:
            runs.append(
                {
                    "status": attempts[-1].get("status"),
                    "elapsed_seconds": sum(float(attempt.get("elapsed_seconds") or 0) for attempt in attempts),
                }
            )
    if not runs:
        return {"attempts": 0, "failure_rate": 0.0, "average_elapsed_seconds": 0.0}

    failures = sum(run["status"] != "success" for run in runs)
    elapsed = sum(run["elapsed_seconds"] for run in runs)
    return {
        "attempts": len(runs),
        "failure_rate": failures / len(runs),
        "average_elapsed_seconds": elapsed / len(runs),
    }


def build_source_priority(source, recent_reports=None):
    from .config import SCRAPE_DIFFICULTY_ORDER, SOURCE_ORDER

    history = source_history(source, recent_reports or [])
    static_difficulty = float(SCRAPE_DIFFICULTY_ORDER.get(source, 50))
    # Higher-risk and slower sources start first so their latency overlaps with
    # faster sources. SOURCE_ORDER remains the deterministic final tie-breaker.
    priority = (
        -history["failure_rate"],
        -history["average_elapsed_seconds"],
        -static_difficulty,
        SOURCE_ORDER.get(source, 999),
    )
    return SourceJob(
        priority=priority,
        source=source,
        reason={
            **history,
            "static_difficulty": int(static_difficulty),
        },
    )


def prioritize_sources(source_names, recent_reports=None):
    queue = [build_source_priority(source, recent_reports) for source in source_names]
    heapq.heapify(queue)
    return [heapq.heappop(queue) for _ in range(len(queue))]
