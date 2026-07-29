from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path


def _finalize_report(entries: list[dict], *, source: str, limit: int) -> dict:
    entries.sort(key=lambda entry: int(entry["bytes"]), reverse=True)
    return {
        "schema_version": 1,
        "source": source,
        "entry_count": len(entries),
        "total_bytes": sum(int(entry["bytes"]) for entry in entries),
        "largest_entries": entries[:limit],
        "entries": entries,
    }


def load_toc(path: Path) -> list[tuple[str, str, str]]:
    value = ast.literal_eval(path.read_text(encoding="utf-8"))
    if not isinstance(value, tuple) or len(value) < 3 or not isinstance(value[2], list):
        raise ValueError("不支援的 PyInstaller PKG TOC 格式")
    return value[2]


def build_report(toc_path: Path, limit: int = 30) -> dict:
    entries = []
    for name, source, item_type in load_toc(toc_path):
        source_path = Path(source)
        size_bytes = source_path.stat().st_size if source_path.is_file() else 0
        entries.append(
            {
                "name": name,
                "type": item_type,
                "source": str(source_path),
                "bytes": size_bytes,
            }
        )
    return _finalize_report(entries, source=str(toc_path), limit=limit)


def build_executable_report(executable_path: Path, limit: int = 30) -> dict:
    from PyInstaller.archive.readers import CArchiveReader

    archive = CArchiveReader(str(executable_path))
    entries = [
        {
            "name": name,
            "type": values[4],
            "bytes": int(values[1]),
            "uncompressed_bytes": int(values[2]),
        }
        for name, values in archive.toc.items()
    ]
    return _finalize_report(entries, source=str(executable_path), limit=limit)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Report the largest PyInstaller package inputs.")
    parser.add_argument("--build-dir", type=Path, default=Path("build"))
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=30)
    args = parser.parse_args(argv)
    if args.executable:
        report = build_executable_report(args.executable, limit=args.limit)
    else:
        toc_files = sorted(args.build_dir.glob("*/PKG-00.toc"))
        if len(toc_files) != 1:
            raise ValueError("預期找到一個 PKG-00.toc，實際為 {}".format(len(toc_files)))
        report = build_report(toc_files[0], limit=args.limit)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
