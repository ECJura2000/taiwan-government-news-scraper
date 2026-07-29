from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import shutil


TOP_LEVEL_DIRECTORIES = (
    "build",
    "dist",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".coverage_html",
    "htmlcov",
    "release-tmp",
    ".release-tmp",
)
TOP_LEVEL_FILES = (
    ".coverage",
    "coverage-package.json",
)
PROTECTED_PARTS = {
    ".git",
    ".venv",
    "新聞搜集區",
    "程式資料",
}


@dataclass(frozen=True)
class CleanupCandidate:
    path: Path
    size_bytes: int


def _path_size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    total = 0
    for child in path.rglob("*"):
        if child.is_file() and not child.is_symlink():
            total += child.stat().st_size
    return total


def _is_safe_candidate(project_root: Path, path: Path) -> bool:
    if path.is_symlink():
        return False
    try:
        relative = path.resolve().relative_to(project_root.resolve())
    except ValueError:
        return False
    return not (set(relative.parts) & PROTECTED_PARTS)


def discover_candidates(project_root: Path) -> list[CleanupCandidate]:
    root = project_root.resolve()
    paths: set[Path] = set()
    for name in TOP_LEVEL_DIRECTORIES:
        candidate = root / name
        if candidate.exists():
            paths.add(candidate)
    for name in TOP_LEVEL_FILES:
        candidate = root / name
        if candidate.is_file():
            paths.add(candidate)
    for candidate in root.rglob("__pycache__"):
        if candidate.is_dir():
            paths.add(candidate)
    release_assets = root / "release-assets"
    if release_assets.is_dir():
        paths.update(release_assets.glob(".release-*.zip"))

    safe_paths = [
        path
        for path in paths
        if _is_safe_candidate(root, path)
        and not any(parent in paths for parent in path.parents if parent != path)
    ]
    return [
        CleanupCandidate(path=path, size_bytes=_path_size(path))
        for path in sorted(safe_paths)
    ]


def remove_candidates(project_root: Path, candidates: list[CleanupCandidate]) -> None:
    root = project_root.resolve()
    for candidate in candidates:
        path = candidate.path
        if not _is_safe_candidate(root, path):
            raise ValueError("拒絕清理不安全路徑：{}".format(path))
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink(missing_ok=True)


def format_size(size_bytes: int) -> str:
    return "{:.2f} MiB".format(size_bytes / (1024 * 1024))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="預覽或清除可重建的建置與測試暫存；預設不刪除任何檔案。",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="實際刪除預覽清單中的安全暫存。",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args(argv)
    candidates = discover_candidates(args.root)
    total = sum(candidate.size_bytes for candidate in candidates)
    action = "將刪除" if args.apply else "可清理"
    for candidate in candidates:
        print("{}  {}".format(format_size(candidate.size_bytes), candidate.path))
    print("{} {}，共 {} 個項目。".format(action, format_size(total), len(candidates)))
    if args.apply:
        remove_candidates(args.root, candidates)
        print("清理完成。")
    else:
        print("目前為預覽模式；確認後加上 --apply 才會刪除。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
