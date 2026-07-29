from __future__ import annotations

import argparse
import json
from pathlib import Path


MINIMUM_ITEM_BYTES = 100 * 1024


def validate_growth(
    current: dict,
    baseline: dict,
    *,
    maximum_growth: float = 0.05,
    allowed_names: set[str] | None = None,
) -> list[str]:
    allowed_names = allowed_names or set()
    baseline_entries = {
        entry["name"]: int(entry["bytes"])
        for entry in baseline.get("entries", [])
        if int(entry.get("bytes", 0)) >= MINIMUM_ITEM_BYTES
    }
    current_entries = {
        entry["name"]: int(entry["bytes"])
        for entry in current.get("entries", [])
    }
    failures = []
    for name, baseline_bytes in baseline_entries.items():
        current_bytes = current_entries.get(name)
        if current_bytes is None or name in allowed_names:
            continue
        growth = current_bytes / baseline_bytes - 1
        if growth > maximum_growth:
            failures.append(
                "{} 成長 {:.2%}（{} -> {} bytes），超過 {:.0%}".format(
                    name,
                    growth,
                    baseline_bytes,
                    current_bytes,
                    maximum_growth,
                )
            )
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Reject unexplained packaged-item growth.")
    parser.add_argument("current", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("--max-growth", type=float, default=0.05)
    parser.add_argument("--allowlist", type=Path)
    args = parser.parse_args(argv)
    current = json.loads(args.current.read_text(encoding="utf-8"))
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    allowed_names: set[str] = set()
    if args.allowlist:
        payload = json.loads(args.allowlist.read_text(encoding="utf-8"))
        allowed = payload.get("allowed", {})
        if not isinstance(allowed, dict) or not all(
            isinstance(name, str) and isinstance(reason, str) and reason.strip()
            for name, reason in allowed.items()
        ):
            raise ValueError("容量成長白名單必須為含非空理由的 allowed mapping")
        allowed_names = set(allowed)
    failures = validate_growth(
        current,
        baseline,
        maximum_growth=args.max_growth,
        allowed_names=allowed_names,
    )
    if failures:
        raise SystemExit("\n".join(failures))
    print("封裝項目未出現未說明的 >{:.0%} 成長。".format(args.max_growth))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
