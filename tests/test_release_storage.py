import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import zipfile


def load_script(name: str):
    path = Path(__file__).resolve().parents[1] / "scripts" / "{}.py".format(name)
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_release_regression_requires_all_platforms_and_fifteen_percent(tmp_path):
    check = load_script("check_release_regression")
    baseline = {
        "assets": {
            platform: {"bytes": 1000}
            for platform in ("linux", "windows", "macos-arm64", "macos-x64")
        }
    }
    for platform, size in (
        ("linux", 850),
        ("windows", 849),
        ("macos-arm64", 800),
        ("macos-x64", 900),
    ):
        (tmp_path / "taiwan-government-news-v1.5.1-{}.zip".format(platform)).write_bytes(
            b"x" * size
        )

    failures = check.validate_release_regression(
        tmp_path,
        baseline,
        version="1.5.1",
        minimum_reduction=0.15,
    )

    assert failures == ["macos-x64 僅縮小 10.00%，低於 15% 門檻"]


def test_source_archive_contains_ai_contract_and_locked_dependencies(tmp_path):
    package_source = load_script("package_source")
    project = tmp_path / "project"
    project.mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    required = set(package_source.REQUIRED_FILES) | {"news_scraper/main.py"}
    for relative in required:
        path = project / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(relative, encoding="utf-8")
    subprocess.run(["git", "add", "."], cwd=project, check=True)

    archive = package_source.build_source_archive(project, tmp_path / "release", "1.5.1")

    with zipfile.ZipFile(archive) as source_zip:
        names = {
            name.removeprefix("taiwan-government-news-v1.5.1-source/")
            for name in source_zip.namelist()
        }
    assert required <= names
    assert all(".venv/" not in name for name in names)


def test_source_packager_cli_runs_from_repository_root(tmp_path):
    project_root = Path(__file__).resolve().parents[1]
    completed = subprocess.run(
        [
            sys.executable,
            "scripts/package_source.py",
            "--output-dir",
            str(tmp_path),
            "--version",
            "1.5.1",
        ],
        cwd=project_root,
        capture_output=True,
        text=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr
    assert (tmp_path / "taiwan-government-news-v1.5.1-source.zip").is_file()


def test_build_size_report_lists_largest_inputs(tmp_path):
    report_module = load_script("report_build_size")
    small = tmp_path / "small.bin"
    large = tmp_path / "large.bin"
    small.write_bytes(b"x" * 10)
    large.write_bytes(b"x" * 20)
    toc = tmp_path / "PKG-00.toc"
    toc.write_text(
        repr(
            (
                str(tmp_path / "app.pkg"),
                {},
                [
                    ("small", str(small), "DATA"),
                    ("large", str(large), "BINARY"),
                ],
            )
        ),
        encoding="utf-8",
    )

    report = report_module.build_report(toc, limit=1)

    assert report["entry_count"] == 2
    assert report["total_bytes"] == 30
    assert report["largest_entries"][0]["name"] == "large"
    assert json.loads(json.dumps(report))["schema_version"] == 1


def test_build_growth_rejects_only_large_unexplained_growth():
    growth = load_script("check_build_growth")
    baseline = {
        "entries": [
            {"name": "large", "bytes": 200_000},
            {"name": "small", "bytes": 10_000},
        ]
    }
    current = {
        "entries": [
            {"name": "large", "bytes": 220_000},
            {"name": "small", "bytes": 20_000},
        ]
    }

    assert growth.validate_growth(current, baseline) == [
        "large 成長 10.00%（200000 -> 220000 bytes），超過 5%"
    ]
    assert growth.validate_growth(current, baseline, allowed_names={"large"}) == []


def test_build_growth_allowlist_requires_documented_reason(tmp_path):
    growth = load_script("check_build_growth")
    current = tmp_path / "current.json"
    baseline = tmp_path / "baseline.json"
    allowlist = tmp_path / "allowlist.json"
    current.write_text('{"entries":[]}', encoding="utf-8")
    baseline.write_text('{"entries":[]}', encoding="utf-8")
    allowlist.write_text('{"allowed":{"base_library.zip":""}}', encoding="utf-8")

    try:
        growth.main([str(current), str(baseline), "--allowlist", str(allowlist)])
    except ValueError as exc:
        assert "非空理由" in str(exc)
    else:
        raise AssertionError("empty allowlist reasons must be rejected")


def test_packaged_report_rejects_pandas_and_numpy():
    forbidden = load_script("check_forbidden_packages")

    assert forbidden.find_forbidden_entries(
        {
            "entries": [
                {"name": "openpyxl/styles.pyc"},
                {"name": "pandas/_libs/window.so"},
                {"name": "numpy.core"},
            ]
        }
    ) == ["numpy", "pandas"]


def test_spec_collects_only_current_selenium_manager_and_excludes_data_stacks():
    project_root = Path(__file__).resolve().parents[1]
    spec = (project_root / "news-scraper.spec").read_text(encoding="utf-8")

    assert 'collect_all("selenium")' not in spec
    assert "selenium_manager_platform" in spec
    assert '"numpy"' in spec
    assert '"pandas"' in spec
    assert "selenium.webdriver.firefox" in spec
    assert "selenium.webdriver.edge" in spec
    assert '"tzdata"' in spec


def test_browser_smoke_uses_local_page_and_headless_chrome():
    project_root = Path(__file__).resolve().parents[1]
    entrypoint = (project_root / "build_entry.py").read_text(encoding="utf-8")

    assert "--browser-smoke-test" in entrypoint
    assert "--headless=new" in entrypoint
    assert "data:text/html" in entrypoint
    assert "browser-smoke" in entrypoint
