# Rust-only v2.1.2 migration

The migration implementation is complete in source: CLI and Tauri share `RunOptions` and the same Rust application service; all 72 sources, browser routes, quality diagnostics, relevance, JSON schema v4 and Excel are native Rust.

The only retained Python file is `scripts/python_compat.py`, a subprocess bridge to `news-scraper`. Legacy scraper packages, tests, dependency locks, build scripts and PyInstaller workflows were removed.

Fixture acceptance preserved titles, dates, departments, summaries, links, relevance output and all 15 workbook fields. Live acceptance reached all 72 sources. Publication is separately gated by the four-platform workflow, dependency audits, archive inspection and release smoke. v2.0.0 remains the rollback release.
