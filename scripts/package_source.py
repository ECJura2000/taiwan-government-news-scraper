from __future__ import annotations

import argparse
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import zipfile

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10
    import tomli as tomllib


REQUIRED_FILES = {
    "AGENTS.md",
    "docs/AI_AUTOMATION.md",
    "pyproject.toml",
    "requirements.lock.txt",
    "requirements-dev.lock.txt",
    "requirements-build.lock.txt",
    "requirements-security.lock.txt",
}


def read_version(project_file: Path) -> str:
    with project_file.open("rb") as stream:
        return str(tomllib.load(stream)["project"]["version"])


def tracked_files(project_root: Path) -> list[Path]:
    completed = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=project_root,
        capture_output=True,
        check=True,
    )
    return [
        project_root / Path(raw.decode("utf-8"))
        for raw in completed.stdout.split(b"\0")
        if raw
    ]


def build_source_archive(project_root: Path, output_dir: Path, version: str) -> Path:
    files = tracked_files(project_root)
    relative_names = {path.relative_to(project_root).as_posix() for path in files}
    missing = sorted(REQUIRED_FILES - relative_names)
    if missing:
        raise ValueError("原始碼 ZIP 缺少必要檔案：{}".format("、".join(missing)))

    archive_name = "taiwan-government-news-v{}-source.zip".format(version)
    root_name = "taiwan-government-news-v{}-source".format(version)
    output_dir.mkdir(parents=True, exist_ok=True)
    archive_path = output_dir / archive_name
    descriptor, temporary_name = tempfile.mkstemp(
        dir=output_dir,
        prefix=".source-",
        suffix=".zip",
    )
    os.close(descriptor)
    temporary_path = Path(temporary_name)
    try:
        with zipfile.ZipFile(
            temporary_path,
            "w",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=9,
            allowZip64=False,
        ) as archive:
            for source in files:
                if not source.is_file():
                    continue
                relative = source.relative_to(project_root).as_posix()
                info = zipfile.ZipInfo.from_file(source, arcname="{}/{}".format(root_name, relative))
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, source.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
        os.replace(temporary_path, archive_path)
    finally:
        temporary_path.unlink(missing_ok=True)
    return archive_path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="建立供 AI agent 與排程使用的固定原始碼 ZIP。")
    parser.add_argument("--output-dir", type=Path, default=Path("release-assets"))
    parser.add_argument("--version")
    args = parser.parse_args(argv)
    project_root = Path(__file__).resolve().parents[1]
    version = args.version or read_version(project_root / "pyproject.toml")
    print(build_source_archive(project_root, args.output_dir, version))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
