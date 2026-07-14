import os
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class WorkspacePaths:
    root: Path
    program_data: Path
    logs: Path
    output: Path
    reports: Path
    used_fallback: bool = False


def is_frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def get_application_root() -> Path:
    if is_frozen():
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[1]


def get_documents_fallback_root() -> Path:
    return Path.home() / "Documents" / "各機關新聞"


def _prepare_root(root: Path) -> WorkspacePaths:
    program_data = root / "程式資料"
    logs = program_data / "logs"
    output = root / "新聞搜集區"
    reports = output / "執行紀錄"
    for directory in (program_data, logs, output, reports):
        directory.mkdir(parents=True, exist_ok=True)

    probe: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=program_data,
            prefix=".write-test-",
            delete=False,
        ) as stream:
            stream.write("ok")
            probe = Path(stream.name)
    finally:
        if probe is not None:
            probe.unlink(missing_ok=True)
    return WorkspacePaths(root, program_data, logs, output, reports)


def prepare_workspace(root: str | Path | None = None) -> WorkspacePaths:
    preferred_root = Path(root).expanduser().resolve() if root is not None else get_application_root()
    try:
        return _prepare_root(preferred_root)
    except OSError:
        fallback = _prepare_root(get_documents_fallback_root())
        return WorkspacePaths(
            fallback.root,
            fallback.program_data,
            fallback.logs,
            fallback.output,
            fallback.reports,
            used_fallback=True,
        )


def get_default_output_dir() -> Path:
    configured = os.environ.get("NEWS_SCRAPER_OUTPUT_DIR", "").strip()
    if configured:
        return Path(configured).expanduser()
    if is_frozen():
        return prepare_workspace().output
    return get_application_root() / "新聞搜集區"
