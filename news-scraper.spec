# -*- mode: python ; coding: utf-8 -*-

import sys
from pathlib import Path

import selenium
from PyInstaller.utils.hooks import collect_all, copy_metadata


def source_module_name(source_path):
    relative_path = source_path.relative_to(Path(SPECPATH)).with_suffix("")
    parts = list(relative_path.parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


datas = [("news_scraper/policy.toml", "news_scraper")]
binaries = []
windows_manifest = str(Path(SPECPATH) / "windows-dpi.manifest") if sys.platform == "win32" else None
scraper_root = Path(SPECPATH) / "news_scraper" / "scrapers" / "ministry"
hiddenimports = sorted(
    {source_module_name(source_path) for source_path in scraper_root.rglob("*.py")}
)
for package_name in ("feedparser",):
    package_datas, package_binaries, package_hiddenimports = collect_all(package_name)
    datas += package_datas
    binaries += package_binaries
    hiddenimports += package_hiddenimports

selenium_root = Path(selenium.__file__).parent
selenium_manager_platform = {
    "darwin": ("macos", "selenium-manager"),
    "linux": ("linux", "selenium-manager"),
    "win32": ("windows", "selenium-manager.exe"),
}[sys.platform]
manager_directory, manager_name = selenium_manager_platform
manager_path = selenium_root / "webdriver" / "common" / manager_directory / manager_name
binaries.append(
    (
        str(manager_path),
        "selenium/webdriver/common/{}".format(manager_directory),
    )
)
datas += copy_metadata("selenium")
hiddenimports += [
    "selenium.common.exceptions",
    "selenium.webdriver.chrome.options",
    "selenium.webdriver.chrome.service",
    "selenium.webdriver.chrome.webdriver",
    "selenium.webdriver.chromium.options",
    "selenium.webdriver.chromium.service",
    "selenium.webdriver.chromium.webdriver",
    "selenium.webdriver.common.by",
    "selenium.webdriver.common.selenium_manager",
    "selenium.webdriver.remote.webdriver",
    "selenium.webdriver.support.expected_conditions",
    "selenium.webdriver.support.ui",
    "tzdata",
]


analysis = Analysis(
    ["build_entry.py"],
    pathex=[],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        "numpy",
        "pandas",
        "pytest",
        "selenium.webdriver.edge",
        "selenium.webdriver.firefox",
        "selenium.webdriver.ie",
        "selenium.webdriver.safari",
        "selenium.webdriver.webkitgtk",
        "selenium.webdriver.wpewebkit",
    ],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(analysis.pure)

executable = EXE(
    pyz,
    analysis.scripts,
    analysis.binaries,
    analysis.datas,
    [],
    name="各機關新聞整理",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    manifest=windows_manifest,
)
