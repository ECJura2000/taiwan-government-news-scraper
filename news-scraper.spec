# -*- mode: python ; coding: utf-8 -*-

from pathlib import Path

from PyInstaller.utils.hooks import collect_all


def source_module_name(source_path):
    relative_path = source_path.relative_to(Path(SPECPATH)).with_suffix("")
    parts = list(relative_path.parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


datas = [("news_scraper/policy.toml", "news_scraper")]
binaries = []
scraper_root = Path(SPECPATH) / "news_scraper" / "scrapers" / "ministry"
hiddenimports = sorted(
    {source_module_name(source_path) for source_path in scraper_root.rglob("*.py")}
)
for package_name in ("feedparser", "selenium"):
    package_datas, package_binaries, package_hiddenimports = collect_all(package_name)
    datas += package_datas
    binaries += package_binaries
    hiddenimports += package_hiddenimports


analysis = Analysis(
    ["build_entry.py"],
    pathex=[],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
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
)
