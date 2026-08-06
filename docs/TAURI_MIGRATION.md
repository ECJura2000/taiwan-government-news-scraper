# Rust/Tauri v2 migration

This branch introduces the v2 desktop shell and the first native Rust scraper primitives.
The Python implementation remains the migration reference until all source adapters and
the report/Excel parity suite pass.

## Current status

- Tauri v2 + Svelte + TypeScript development and production builds work on macOS Apple Silicon.
- The Rust core has tested contracts for news items, RSS/HTML parsing, HTTP error mapping,
  deduplication, source scheduling, and retry classification.
- The desktop command surface is `list_sources`, `run_scrape`, and `cancel_run`.
- The current `run_scrape` command uses the Python implementation as a temporary compatibility
  engine. It is not a v2 release gate and must be replaced by native Rust before release.

## Release gate

v2.0.0 must not be tagged until all existing source adapters, Excel output, JSON report fields,
relevance policy migration, GUI parity, four-platform bundles, and Python/Rust dual-run tests
have passed. v1.6.0 remains the rollback release during the v2.0.0 migration.
