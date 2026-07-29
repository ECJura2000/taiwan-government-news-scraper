import importlib.util
from pathlib import Path
import sys


def load_cleanup_module():
    path = Path(__file__).resolve().parents[1] / "scripts" / "clean_workspace.py"
    spec = importlib.util.spec_from_file_location("clean_workspace", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_cleanup_preview_preserves_protected_data(tmp_path):
    cleanup = load_cleanup_module()
    (tmp_path / "build").mkdir()
    (tmp_path / "build" / "artifact.bin").write_bytes(b"x" * 100)
    (tmp_path / "news_scraper" / "__pycache__").mkdir(parents=True)
    (tmp_path / "news_scraper" / "__pycache__" / "module.pyc").write_bytes(b"x" * 20)
    protected = (
        tmp_path / ".venv",
        tmp_path / ".git",
        tmp_path / "新聞搜集區",
        tmp_path / "程式資料",
    )
    for directory in protected:
        directory.mkdir()
        (directory / "keep.txt").write_text("keep", encoding="utf-8")

    candidates = cleanup.discover_candidates(tmp_path)
    paths = {candidate.path.relative_to(tmp_path).as_posix() for candidate in candidates}

    assert paths == {"build", "news_scraper/__pycache__"}
    assert sum(candidate.size_bytes for candidate in candidates) == 120
    assert all((directory / "keep.txt").exists() for directory in protected)


def test_cleanup_requires_apply_and_removes_only_discovered_candidates(tmp_path):
    cleanup = load_cleanup_module()
    cache = tmp_path / ".pytest_cache"
    cache.mkdir()
    (cache / "state").write_text("temporary", encoding="utf-8")
    output = tmp_path / "新聞搜集區"
    output.mkdir()
    workbook = output / "本週新聞整理.xlsx"
    workbook.write_text("important", encoding="utf-8")

    assert cleanup.main(["--root", str(tmp_path)]) == 0
    assert cache.exists()
    assert cleanup.main(["--root", str(tmp_path), "--apply"]) == 0

    assert not cache.exists()
    assert workbook.exists()


def test_cleanup_skips_symlinks(tmp_path):
    cleanup = load_cleanup_module()
    external = tmp_path.parent / "{}-external".format(tmp_path.name)
    external.mkdir(exist_ok=True)
    (external / "keep.txt").write_text("keep", encoding="utf-8")
    (tmp_path / "build").symlink_to(external, target_is_directory=True)

    candidates = cleanup.discover_candidates(tmp_path)

    assert candidates == []
    assert (external / "keep.txt").exists()
