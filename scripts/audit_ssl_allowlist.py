from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
import json
from pathlib import Path
import socket

import requests


def probe_host(host: str, timeout: float) -> dict:
    url = "https://{}/".format(host)
    try:
        with requests.get(
            url,
            timeout=timeout,
            allow_redirects=False,
            stream=True,
        ) as response:
            return {
                "host": host,
                "success": True,
                "status_code": response.status_code,
                "error": "",
            }
    except requests.RequestException as exc:
        return {
            "host": host,
            "success": False,
            "status_code": None,
            "error": "{}: {}".format(type(exc).__name__, exc),
        }


def run_audit(candidates: list[str], *, timeout: float, workers: int, environment: str) -> dict:
    results = []
    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {
            executor.submit(probe_host, host, timeout): host
            for host in candidates
        }
        for future in as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda item: item["host"])
    return {
        "schema_version": 1,
        "environment": environment,
        "hostname": socket.gethostname(),
        "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "candidate_count": len(candidates),
        "success_count": sum(bool(item["success"]) for item in results),
        "results": results,
    }


def approved_intersection(local: dict, github: dict) -> list[str]:
    local_success = {
        item["host"]
        for item in local.get("results", [])
        if item.get("success") is True
    }
    github_success = {
        item["host"]
        for item in github.get("results", [])
        if item.get("success") is True
    }
    return sorted(local_success & github_success)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Audit SSL fallback removal candidates.")
    parser.add_argument("--candidates", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--environment", default="local")
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--compare", nargs=2, type=Path)
    args = parser.parse_args(argv)
    if args.compare:
        local = json.loads(args.compare[0].read_text(encoding="utf-8"))
        github = json.loads(args.compare[1].read_text(encoding="utf-8"))
        approved = approved_intersection(local, github)
        payload = {
            "schema_version": 1,
            "local_evidence": str(args.compare[0]),
            "github_evidence": str(args.compare[1]),
            "approved_removals": approved,
        }
    else:
        if args.candidates is None:
            parser.error("--candidates is required unless --compare is used")
        candidate_payload = json.loads(args.candidates.read_text(encoding="utf-8"))
        candidates = [str(host) for host in candidate_payload.get("candidates", [])]
        payload = run_audit(
            candidates,
            timeout=args.timeout,
            workers=max(1, args.workers),
            environment=args.environment,
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
