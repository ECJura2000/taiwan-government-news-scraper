from __future__ import annotations

import argparse
import hashlib
import os
import stat
import tempfile
import zipfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10
    import tomli as tomllib

from news_scraper.io_utils import atomic_write_text


ROOT_FOLDER = "各機關新聞"
EXECUTABLE_NAME = "各機關新聞整理"
EMPTY_DIRECTORIES = ("程式資料/", "程式資料/logs/", "新聞搜集區/", "新聞搜集區/執行紀錄/")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_version(project_file: Path) -> str:
    with project_file.open("rb") as stream:
        return str(tomllib.load(stream)["project"]["version"])


def add_directory(archive: zipfile.ZipFile, relative_path: str) -> None:
    info = zipfile.ZipInfo("{}/{}".format(ROOT_FOLDER, relative_path))
    info.external_attr = (stat.S_IFDIR | 0o755) << 16
    archive.writestr(info, b"")


def add_file(archive: zipfile.ZipFile, source: Path, relative_path: str, executable: bool = False) -> None:
    info = zipfile.ZipInfo.from_file(source, arcname="{}/{}".format(ROOT_FOLDER, relative_path))
    mode = 0o755 if executable else 0o644
    info.external_attr = (stat.S_IFREG | mode) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(info, source.read_bytes())


def build_archive(
    *,
    project_root: Path,
    dist_dir: Path,
    output_dir: Path,
    platform: str,
    version: str,
    max_executable_mib: float,
) -> Path:
    executable_file = EXECUTABLE_NAME + (".exe" if platform == "windows" else "")
    executable_path = dist_dir / executable_file
    if not executable_path.is_file():
        raise FileNotFoundError("找不到封裝執行檔：{}".format(executable_path))

    executable_size_mib = executable_path.stat().st_size / (1024 * 1024)
    if executable_size_mib > max_executable_mib:
        raise ValueError("執行檔 {:.2f} MiB 超過 {:.2f} MiB 上限".format(executable_size_mib, max_executable_mib))

    readme_path = project_root / "docs" / "PORTABLE_README.txt"
    manifest_text = "{}  {}\n{}  {}\n".format(
        sha256(executable_path),
        executable_file,
        sha256(readme_path),
        "使用說明.txt",
    )
    output_dir.mkdir(parents=True, exist_ok=True)
    archive_path = output_dir / "taiwan-government-news-v{}-{}.zip".format(version, platform)
    descriptor, temporary_name = tempfile.mkstemp(dir=output_dir, prefix=".release-", suffix=".zip")
    os.close(descriptor)
    temporary_path = Path(temporary_name)
    try:
        with zipfile.ZipFile(temporary_path, "w", allowZip64=False) as archive:
            add_directory(archive, "")
            for directory in EMPTY_DIRECTORIES:
                add_directory(archive, directory)
            add_file(archive, executable_path, executable_file, executable=True)
            add_file(archive, readme_path, "使用說明.txt")
            manifest_info = zipfile.ZipInfo("{}/SHA256SUMS.txt".format(ROOT_FOLDER))
            manifest_info.external_attr = (stat.S_IFREG | 0o644) << 16
            manifest_info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(manifest_info, manifest_text.encode("utf-8"))
        os.replace(temporary_path, archive_path)
    finally:
        temporary_path.unlink(missing_ok=True)

    size_manifest = archive_path.with_name(archive_path.name + "-SIZE-MANIFEST.txt")
    atomic_write_text(
        size_manifest,
        "archive={}\narchive_bytes={}\nexecutable_bytes={}\n".format(
            archive_path.name,
            archive_path.stat().st_size,
            executable_path.stat().st_size,
        ),
    )
    return archive_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="建立各機關新聞跨平台可攜 ZIP。")
    parser.add_argument("--platform", required=True, choices=("linux", "windows", "macos"))
    parser.add_argument("--dist-dir", type=Path, default=Path("dist"))
    parser.add_argument("--output-dir", type=Path, default=Path("release-assets"))
    parser.add_argument("--version")
    parser.add_argument("--max-executable-mib", type=float, default=90)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    project_root = Path(__file__).resolve().parents[1]
    version = args.version or read_version(project_root / "pyproject.toml")
    archive = build_archive(
        project_root=project_root,
        dist_dir=args.dist_dir,
        output_dir=args.output_dir,
        platform=args.platform,
        version=version,
        max_executable_mib=args.max_executable_mib,
    )
    print(archive)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
