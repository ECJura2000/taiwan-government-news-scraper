import json
import socket
import subprocess
import sys
import types
from datetime import datetime
from pathlib import Path

import pytest

from news_scraper.application import RunOptions, RunResult, run_news_scraper
from news_scraper.gui import (
    GuiSettings,
    enable_windows_dpi_awareness,
    get_windows_dpi_awareness,
    load_settings,
    save_settings,
)
from news_scraper.io_utils import atomic_write_text
from news_scraper.main import parse_args
from news_scraper.paths import WorkspacePaths
from news_scraper.run_lock import RunAlreadyActiveError, RunLock


def test_atomic_write_text_replaces_existing_file(tmp_path):
    destination = tmp_path / "report.json"
    destination.write_text("old", encoding="utf-8")

    atomic_write_text(destination, "new")

    assert destination.read_text(encoding="utf-8") == "new"
    assert list(tmp_path.glob(".report.json-*.tmp")) == []


def test_run_lock_rejects_active_process_and_recovers_stale_lock(tmp_path):
    lock_path = tmp_path / "run.lock"
    with RunLock(lock_path, mode="test"):
        with pytest.raises(RunAlreadyActiveError):
            RunLock(lock_path, mode="second").acquire()

    lock_path.write_text(
        json.dumps(
            {
                "pid": 99999999,
                "hostname": socket.gethostname(),
                "started_at": datetime.now().isoformat(),
                "mode": "stale",
            }
        ),
        encoding="utf-8",
    )
    with RunLock(lock_path, mode="replacement"):
        assert json.loads(lock_path.read_text(encoding="utf-8"))["mode"] == "replacement"
    assert not lock_path.exists()


def test_gui_settings_persist_only_declared_non_secret_fields(tmp_path):
    path = tmp_path / "settings.json"
    save_settings(
        path,
        GuiSettings(sources=["行政院"], output_dir=str(tmp_path), max_workers=3),
    )
    payload = json.loads(path.read_text(encoding="utf-8"))

    assert payload["sources"] == ["行政院"]
    assert payload["schema_version"] == 2
    assert "fail_on_source_error" not in payload
    assert "webhook" not in payload
    assert load_settings(path, ["行政院", "財政部"]).max_workers == 3


def test_gui_settings_migrate_schema_one_and_ignore_removed_strict_flag(tmp_path):
    path = tmp_path / "settings.json"
    path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "sources": ["行政院"],
                "fail_on_source_error": True,
                "max_workers": 4,
            }
        ),
        encoding="utf-8",
    )

    settings = load_settings(path, ["行政院", "財政部"])

    assert settings.schema_version == 2
    assert settings.sources == ["行政院"]
    assert settings.max_workers == 4
    assert not hasattr(settings, "fail_on_source_error")


def test_invalid_gui_numeric_settings_fall_back_safely(tmp_path):
    path = tmp_path / "settings.json"
    path.write_text(
        json.dumps({"sources": ["行政院"], "max_workers": "bad", "report_retention_days": None}),
        encoding="utf-8",
    )

    settings = load_settings(path, ["行政院"])

    assert settings.max_workers == 8
    assert settings.report_retention_days == 180


def test_windows_dpi_awareness_prefers_per_monitor_v2(monkeypatch):
    import news_scraper.gui as gui

    calls = []
    ctypes_stub = types.ModuleType("ctypes")
    ctypes_stub.c_void_p = lambda value: value
    ctypes_stub.windll = types.SimpleNamespace(
        user32=types.SimpleNamespace(
            SetProcessDpiAwarenessContext=lambda context: calls.append(context) or True,
        ),
    )
    monkeypatch.setattr(gui.sys, "platform", "win32")
    monkeypatch.setitem(sys.modules, "ctypes", ctypes_stub)

    assert enable_windows_dpi_awareness() is True
    assert calls == [-4]


def test_windows_dpi_awareness_falls_back_for_older_windows(monkeypatch):
    import news_scraper.gui as gui

    calls = []
    ctypes_stub = types.ModuleType("ctypes")
    ctypes_stub.c_void_p = lambda value: value
    ctypes_stub.windll = types.SimpleNamespace(
        user32=types.SimpleNamespace(),
        shcore=types.SimpleNamespace(
            SetProcessDpiAwareness=lambda awareness: calls.append(awareness) or 0,
        ),
    )
    monkeypatch.setattr(gui.sys, "platform", "win32")
    monkeypatch.setitem(sys.modules, "ctypes", ctypes_stub)

    assert enable_windows_dpi_awareness() is True
    assert calls == [2]


def test_windows_dpi_awareness_query_reports_per_monitor(monkeypatch):
    import news_scraper.gui as gui

    class StubFunction:
        def __init__(self, callback):
            self.callback = callback

        def __call__(self, *args):
            return self.callback(*args)

    ctypes_stub = types.ModuleType("ctypes")
    ctypes_stub.c_int = int
    ctypes_stub.c_void_p = lambda value=None: value
    ctypes_stub.windll = types.SimpleNamespace(
        user32=types.SimpleNamespace(
            GetThreadDpiAwarenessContext=StubFunction(lambda: "context"),
            GetAwarenessFromDpiAwarenessContext=StubFunction(
                lambda context: 2 if context == "context" else -1
            ),
        ),
    )
    monkeypatch.setattr(gui.sys, "platform", "win32")
    monkeypatch.setitem(sys.modules, "ctypes", ctypes_stub)

    assert get_windows_dpi_awareness() == 2


def test_windows_gui_smoke_rejects_non_per_monitor_awareness(monkeypatch, capsys):
    import news_scraper.gui as gui

    monkeypatch.setattr(gui.sys, "platform", "win32")
    monkeypatch.setattr(gui, "enable_windows_dpi_awareness", lambda: True)
    monkeypatch.setattr(gui, "get_windows_dpi_awareness", lambda: 1)

    assert gui.main(smoke_test=True) == 1
    assert "Per-Monitor DPI awareness" in capsys.readouterr().err


def test_windows_manifest_declares_per_monitor_v2():
    project_root = Path(__file__).resolve().parents[1]
    manifest = (project_root / "windows-dpi.manifest").read_text(encoding="utf-8")
    spec = (project_root / "news-scraper.spec").read_text(encoding="utf-8")

    assert "PerMonitorV2, PerMonitor" in manifest
    assert "<dpiAware" in manifest
    assert "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" in manifest
    assert "manifest=windows_manifest" in spec


def test_gui_uses_named_fonts_instead_of_tk_defaults():
    project_root = Path(__file__).resolve().parents[1]
    gui_source = (project_root / "news_scraper" / "gui.py").read_text(encoding="utf-8")

    assert "TkDefaultFont" not in gui_source
    assert "TkFixedFont" not in gui_source
    assert "NewsScraperCJK" in gui_source
    assert "NewsScraperLatin" in gui_source
    assert "Times New Roman" in gui_source
    assert "DFKai-SB" in gui_source


def test_relevance_profile_cli_flags_are_mutually_exclusive():
    args = parse_args(["--relevance-profile", "custom.json"])

    assert args.relevance_profile == "custom.json"
    assert args.no_relevance_profile is False
    with pytest.raises(SystemExit):
        parse_args(["--relevance-profile", "custom.json", "--no-relevance-profile"])


def test_headless_source_listing_does_not_import_tkinter():
    project_root = Path(__file__).resolve().parents[1]
    script = (
        "import sys; from news_scraper.main import main; "
        "code=main(['--list-sources']); "
        "print('TK_LOADED=' + str('tkinter' in sys.modules)); raise SystemExit(code)"
    )
    completed = subprocess.run(
        [sys.executable, "-c", script],
        cwd=project_root,
        env={"PATH": "", "DISPLAY": "", "WAYLAND_DISPLAY": ""},
        capture_output=True,
        text=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr
    assert "TK_LOADED=False" in completed.stdout


def test_run_result_json_summary_uses_stable_machine_fields(tmp_path):
    result = RunResult(
        status="attention",
        output_path=tmp_path / "weekly.xlsx",
        report_path=tmp_path / "run.json",
        news_count=4,
        failed_sources=("工程會",),
        quality={"output_count": 4, "alert_reasons": []},
    )

    assert result.to_summary() == {
        "status": "attention",
        "output_file": str(tmp_path / "weekly.xlsx"),
        "report_file": str(tmp_path / "run.json"),
        "news_count": 4,
        "failed_sources": ["工程會"],
        "anomalies": [],
        "duration_seconds": 0.0,
        "error_counts": {},
        "parser_warnings": [],
        "relevance_policy": {},
        "quality": {"output_count": 4, "alert_reasons": []},
        "insecure_ssl_hosts": [],
        "cancelled": False,
    }


def test_run_result_json_summary_compacts_quality_issues(tmp_path):
    result = RunResult(
        status="attention",
        output_path=None,
        report_path=tmp_path / "run.json",
        news_count=0,
        quality={"output_count": 0, "issues": [{"category": "duplicate"}] * 20},
    )

    assert result.to_summary()["quality"] == {"output_count": 0, "issue_count": 20}


def test_cancelled_application_writes_report_without_excel(monkeypatch, tmp_path):
    import threading

    import news_scraper.application as application
    import news_scraper.main as main_module
    import news_scraper.runtime as runtime

    workspace = WorkspacePaths(
        root=tmp_path,
        program_data=tmp_path / "程式資料",
        logs=tmp_path / "程式資料" / "logs",
        output=tmp_path / "新聞搜集區",
        reports=tmp_path / "新聞搜集區" / "執行紀錄",
    )
    for directory in (workspace.program_data, workspace.logs, workspace.output, workspace.reports):
        directory.mkdir(parents=True, exist_ok=True)
    cancel_event = threading.Event()

    monkeypatch.setattr(application, "prepare_workspace", lambda: workspace)
    monkeypatch.setattr(runtime, "validate_runtime_environment", lambda **kwargs: True)
    monkeypatch.setattr(main_module, "normalize_selected_sources", lambda sources: ["行政院"])

    def cancel_during_collection(**kwargs):
        cancel_event.set()
        kwargs["context"].cancelled = True
        return []

    monkeypatch.setattr(main_module, "collect_all_this_week_news", cancel_during_collection)

    result = run_news_scraper(
        RunOptions(sources=("行政院",), output_dir=workspace.output),
        cancel_event=cancel_event,
    )

    assert result.status == "cancelled"
    assert result.output_path is None
    assert result.report_path is not None and result.report_path.exists()
    assert json.loads(result.report_path.read_text(encoding="utf-8"))["output_file"] == ""
    assert list(workspace.output.glob("*.xlsx")) == []
    assert not (workspace.output / ".news-scraper.run.lock").exists()


def test_gui_and_headless_runs_use_the_same_relevance_hash(monkeypatch, tmp_path):
    import threading

    import news_scraper.application as application
    import news_scraper.main as main_module
    import news_scraper.runtime as runtime
    from news_scraper.relevance import (
        KeywordRule,
        RelevanceProfile,
        TopicRule,
        get_effective_relevance_hash,
        save_relevance_profile,
    )

    workspace = WorkspacePaths(
        root=tmp_path,
        program_data=tmp_path / "程式資料",
        logs=tmp_path / "程式資料" / "logs",
        output=tmp_path / "新聞搜集區",
        reports=tmp_path / "新聞搜集區" / "執行紀錄",
    )
    for directory in (
        workspace.program_data,
        workspace.logs,
        workspace.output,
        workspace.reports,
    ):
        directory.mkdir(parents=True, exist_ok=True)
    profile = RelevanceProfile(
        name="跨入口設定",
        topics=[
            TopicRule(
                id="topic:cyber",
                name="資安",
                core_keywords=[KeywordRule(id="keyword:cyber", text="零信任")],
            )
        ],
    )
    profile_path = workspace.program_data / "relevance-profile.json"
    save_relevance_profile(profile_path, profile)

    monkeypatch.setattr(application, "prepare_workspace", lambda: workspace)
    monkeypatch.setattr(runtime, "validate_runtime_environment", lambda **kwargs: True)
    monkeypatch.setattr(main_module, "normalize_selected_sources", lambda sources: ["行政院"])

    def cancel_during_collection(**kwargs):
        kwargs["cancel_event"].set()
        kwargs["context"].cancelled = True
        return []

    monkeypatch.setattr(main_module, "collect_all_this_week_news", cancel_during_collection)

    hashes = []
    for mode in ("gui", "headless"):
        output_dir = tmp_path / mode
        result = run_news_scraper(
            RunOptions(
                sources=("行政院",),
                output_dir=output_dir,
                relevance_profile_path=profile_path,
                mode=mode,
            ),
            cancel_event=threading.Event(),
        )
        assert result.report_path is not None
        report = json.loads(result.report_path.read_text(encoding="utf-8"))
        hashes.append(report["relevance_policy"]["ruleset_hash"])

    assert hashes == [get_effective_relevance_hash(profile)] * 2


@pytest.mark.parametrize(
    ("platform", "executable_name"),
    [
        ("linux", "各機關新聞整理"),
        ("windows", "各機關新聞整理.exe"),
        ("macos-arm64", "各機關新聞整理"),
        ("macos-x64", "各機關新聞整理"),
    ],
)
def test_portable_archive_has_expected_unicode_layout_and_permissions(tmp_path, platform, executable_name):
    from scripts.package_release import build_archive

    project_root = Path(__file__).resolve().parents[1]
    dist_dir = tmp_path / "dist"
    output_dir = tmp_path / "release"
    dist_dir.mkdir()
    executable = dist_dir / executable_name
    executable.write_bytes(b"binary")

    archive_path = build_archive(
        project_root=project_root,
        dist_dir=dist_dir,
        output_dir=output_dir,
        platform=platform,
        version="1.2.0",
        max_executable_mib=1,
    )

    import zipfile

    assert archive_path.name == "taiwan-government-news-v1.2.0-{}.zip".format(platform)
    with zipfile.ZipFile(archive_path) as archive:
        names = archive.namelist()
        executable_info = archive.getinfo("各機關新聞/{}".format(executable_name))
        assert "各機關新聞/新聞搜集區/執行紀錄/" in names
        assert "各機關新聞/程式資料/logs/" in names
        assert executable_info.external_attr >> 16 & 0o111
        assert "各機關新聞/SHA256SUMS.txt" in names
        assert "各機關新聞/啟動圖解/Windows.svg" in names
        assert "各機關新聞/啟動圖解/macOS.svg" in names
