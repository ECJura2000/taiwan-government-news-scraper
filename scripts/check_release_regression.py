from __future__ import annotations

import argparse
import json
from pathlib import Path


PLATFORMS = ("linux", "windows", "macos-arm64", "macos-x64")


def validate_release_regression(
    release_dir: Path,
    baseline: dict,
    *,
    version: str,
    minimum_reduction: float,
) -> list[str]:
    failures = []
    for platform in PLATFORMS:
        path = release_dir / "taiwan-government-news-v{}-{}.zip".format(version, platform)
        if not path.is_file():
            failures.append("缺少平台 ZIP：{}".format(path.name))
            continue
        baseline_bytes = int(baseline["assets"][platform]["bytes"])
        current_bytes = path.stat().st_size
        reduction = 1 - current_bytes / baseline_bytes
        if reduction + 1e-9 < minimum_reduction:
            failures.append(
                "{} 僅縮小 {:.2%}，低於 {:.0%} 門檻".format(
                    platform,
                    reduction,
                    minimum_reduction,
                )
            )
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Compare portable ZIPs with the v1.5.0 baseline.")
    parser.add_argument("release_dir", type=Path)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--minimum-reduction", type=float, default=0.15)
    args = parser.parse_args(argv)
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    failures = validate_release_regression(
        args.release_dir,
        baseline,
        version=args.version,
        minimum_reduction=args.minimum_reduction,
    )
    if failures:
        raise SystemExit("\n".join(failures))
    print("四平台 ZIP 均較 {} 縮小至少 {:.0%}。".format(
        baseline["version"],
        args.minimum_reduction,
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
