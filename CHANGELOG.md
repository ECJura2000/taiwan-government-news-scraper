# Changelog

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
