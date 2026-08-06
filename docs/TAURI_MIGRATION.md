# Rust/Tauri v2 migration

This branch is the native Rust migration line. The Python implementation remains available
only as an explicit compatibility fallback while source-by-source parity is completed.

## Current status

- Tauri v2 + Svelte + TypeScript development and production builds work on macOS Apple Silicon.
- The Rust core has tested contracts for news items, RSS/HTML parsing, HTTP error mapping,
  deduplication, source scheduling, and retry classification.
- The desktop command surface is `list_sources`, `run_scrape`, and `cancel_run`.
- CLI and Tauri `run_scrape` now execute the Rust native engine. They do not spawn Python.
- `scripts/python_compat.py` is the only supported explicit legacy bridge and is reachable from
  the Rust CLI with `--python-compat`; it is not part of the normal execution path.
- The first native engine milestone includes the 72-source catalog, concurrent HTTP fetching,
  generic RSS/HTML parsing, current-week filtering, deduplication, JSON reports, and Excel output.
- Source-specific parser behavior, schema-v4 observability, relevance-policy parity, browser
  routes, and the full GUI parity suite remain migration work and are not yet release-ready.

## Release gate

The migration release must not be tagged until all existing source adapters, Excel output, JSON
report fields, relevance policy migration, GUI parity, four-platform bundles, and Python/Rust
dual-run tests have passed. The published v2.0.0 Python-backed release remains the rollback
release during this migration.
