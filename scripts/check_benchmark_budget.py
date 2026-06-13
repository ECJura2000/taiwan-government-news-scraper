import json
import sys


payload = json.load(open(sys.argv[1], encoding="utf-8"))
baseline = json.load(open(sys.argv[2], encoding="utf-8")) if len(sys.argv) > 2 else None
budgets = {"1000": 150.0, "10000": 1200.0}
for size, limit in budgets.items():
    if size not in payload["dedupe_quality"]:
        continue
    actual = float(payload["dedupe_quality"][size]["p95_ms"])
    if actual > limit:
        print(f"::warning::Taiwan quality P95 {size} rows is {actual:.3f} ms, budget {limit:.3f} ms")
    if baseline and size in baseline.get("dedupe_quality", {}):
        previous = float(baseline["dedupe_quality"][size]["p95_ms"])
        if previous and actual > previous * 1.30:
            print(f"::warning::Taiwan quality P95 {size} rows regressed more than 30%")
