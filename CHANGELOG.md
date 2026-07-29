# Changelog

## Unreleased

## 1.5.2 - 2026-07-29

- Added a native topic color picker with a synchronized hex field, live color preview, and immediate invalid-color feedback.
- Changed the GUI execution log to use the same Traditional Chinese named font as buttons, labels, and topic controls.
- Expanded packaged GUI smoke coverage to open the topic dialog, simulate a palette choice, verify the preview, and reject high-DPI layout overflow.

## 1.5.1 - 2026-07-29

- Replaced the pandas-backed Excel path with native rows and openpyxl while preserving workbook data, ordering, colors, fonts, hyperlinks, validation, filters, and frozen panes.
- Removed pandas and numpy from runtime dependencies, locked environments, runtime checks, and portable executables; the optional DataFrame compatibility wrapper remains available only when callers install pandas themselves.
- Reduced Selenium packaging to Chrome/Chromium, Remote WebDriver dependencies, the current platform Selenium Manager, and required license metadata, with an offline real-browser smoke test.
- Added v1.5.0 four-platform ZIP and packaged-module baselines, a 15% ZIP reduction requirement, a 65 MiB executable limit, and a 170 MiB total Release budget.
- Added a dedicated fixed source ZIP containing `AGENTS.md`, the AI automation guide, complete tracked source, and hash-locked dependency files.
- Added a dry-run-first cleanup tool that only removes rebuildable build and test caches, plus a ten-backup limit for imported relevance profiles.
- Corrected `slow_run` detection to use wall-clock duration and at least three normal runs with the exact same source set.
- Preserved complete source-smoke reports and stderr, retried first failures after ten seconds, and distinguished unstable retry success from final failure.
- Added stable fixtures and 80% per-file coverage gates for Corrections Agency, Agricultural Finance Agency, Fisheries Agency, Social and Family Affairs Administration, Ministry of Environment, and Public Construction Commission parsers.
- Removed the ineffective strict-failure GUI option, migrated GUI settings to schema 2, and expanded machine-readable summaries with duration, errors, parser warnings, and relevance policy details.

## 1.5.0 - 2026-07-28

- Replaced the fixed AI initiative classifier with a reusable topic relevance engine while preserving the existing AI Ten Major Initiatives as the initial template.
- Added a four-tab GUI editor for freely creating, duplicating, renaming, ordering, disabling, deleting, importing, exporting, and testing topics and keywords.
- Added core, supporting, context, global exclusion, and topic exclusion rules with title/summary field selection, stable IDs, tombstones, validation, and deterministic profile hashes.
- Shared the saved relevance profile across GUI, headless, and scheduled runs, with explicit CLI override and built-in-profile options.
- Generalized Excel columns and reference sheets, and added effective rule/version auditing to Excel and schema-v3 JSON reports.
- Standardized GUI typography on Kai-style Traditional Chinese fonts and Times New Roman-compatible Latin fonts with dynamic high-DPI scaling.

## 1.4.1 - 2026-07-28

- Added a Per-Monitor V2 Windows manifest so high-DPI displays render the GUI without bitmap blur.
- Added runtime DPI-awareness fallbacks for older supported Windows environments.

## 1.4.0 - 2026-07-28

- Added a prominent latest-release download table, real GUI and Excel previews, and platform-specific first-run guides.
- Split macOS portable builds into explicit Apple Silicon and Intel archives.
- Simplified public Release assets while retaining per-platform size manifests as short-lived Actions artifacts.
- Expanded generated Release notes with exact download choices and first-run requirements.
- Consolidated pending Python and GitHub Actions dependency updates.

## 1.3.0 - 2026-07-23

- Added layered Taipei Veterans General Hospital fetching with Cloudflare detection, Selenium fallback, official-homepage fallback, and parser fixtures.
- Added structured curl transport errors for timeout, HTTP, SSL, and connection failures while suppressing noisy progress output and tolerating isolated invalid UTF-8 bytes.
- Added schema-v2 weekly reports with explicit week windows, distinct-week zero-item evidence, and source-combination-aware summary coverage trend alerts.
- Expanded Mypy to the complete `news_scraper` package and added per-module coverage gates for HTTP, monitoring, main, and the Taipei Veterans General Hospital scraper.
- Replaced URL-only scheduled smoke checks with rotating live parser runs covering every source and fixed high-risk sources.
- Consolidated pending Dependabot updates for development dependencies and GitHub Actions.

## 1.2.1 - 2026-07-15

- Added native Windows and Linux live smoke tests for published release executables.
- Fixed UTF-8 portable archive extraction on Windows GitHub runners.
- Preserved Linux executable permissions after portable archive extraction.
- Separated package/runtime failures from external live-source connectivity warnings while retaining Excel, JSON, and log evidence.

## 1.2.0 - 2026-07-14

- Added a cross-platform tkinter GUI while preserving the existing headless `python -m news_scraper` automation contract.
- Added a shared application service, cooperative cancellation, stable JSON summaries, portable workspace paths, and cross-process run locking.
- Made Excel, JSON, settings, and release manifests atomic to prevent partial or overlapping output.
- Added one-file GUI/headless PyInstaller builds and portable ZIP layouts for Linux, Windows, and macOS.
- Changed releases to an explicit one-click workflow with immutable tags, pinned Action SHAs, checksums, and size gates.
- Added hash-locked runtime, development, build, and security environments.
- Upgraded vulnerable HTTP dependencies and replaced external RSS XML parsing with `defusedxml`.
- Added Bandit, pip-audit, workflow pinning, GUI routing, cancellation, locking, and portable archive tests.

## 1.1.3 - 2026-07-13

- Added the missing DGPA news source and full-name source aliases.
- Added versioned, validated AI Ten Major Initiatives rules with summary-aware scoring, negative terms, reasons, and Excel metadata.
- Added a 205-row labeled historical-title corpus with precision/recall budgets and a repeatable evaluation command.
- Fixed timezone-aware RSS dates to use Asia/Taipei and changed SSL fallback to prefer verified curl before allowlisted insecure requests.
- Prevented out-of-order RSS entries from hiding later current-week news and added publication-date provenance.
- Restricted insecure SSL redirects to allowlisted hosts on every hop and removed global TLS-warning suppression.
- Added primary-agency detail summaries, summary coverage metrics, and normalized HTML summary text.
- Added per-initiative scores, a frozen temporal holdout corpus, stricter corpus validation, and optional live source-title verification.
- Fixed direct terminal execution and isolated Selenium or unexpected source failures.
- Added automatic scraper submodule collection, packaged-runtime checks, and release size budgets.

## 1.0.0 - 2026-06-14

- Added typed models, source validation, observability budgets, schema validation, and selective retry.
- Added integration, fault-injection, property, coverage, security, benchmark, and source smoke checks.
- Added formal project delivery, UAT, long-term-run, and disaster-recovery procedures.

## 0.3.0 - 2026-06-11

- Added Python 3.10, 3.12, and 3.13 CI compatibility testing.
- Added typed scraper registry contracts and expanded static type checking.
- Added observable parser warnings to run reports and alert decisions.
- Added MIT License and security reporting policy.

## 0.2.0 - 2026-06-09

- Added structured JSON run reports, trend summaries, anomaly detection, alert hooks, and report retention.
- Added news quality validation, non-news filtering, duplicate handling, and externally configurable TOML policies.
- Hardened SSL fallback behavior so insecure requests are limited to explicitly allowed hosts and audited per run.
- Improved Excel news-link display so labels remain plain text while URLs are visually identified as links.
- Expanded parser fixtures, integration coverage, static type checking, dependency locks, and GitHub Actions checks.

## 0.1.0

- Initial packaged weekly Taiwan government news scraper.
