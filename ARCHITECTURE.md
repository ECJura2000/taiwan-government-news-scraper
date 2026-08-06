# Architecture

The CLI and Tauri commands both call `native::run(RunOptions, cancellation_flag)`. That service resolves the Taipei date range, schedules selected sources, applies route fallback, filters and validates records, evaluates the embedded relevance policy, and writes schema-v4 JSON plus the parity-compatible workbook.

- `src-tauri/resources/sources.json`: declarative 72-source catalog and official routes.
- `src-tauri/src/scraper/`: HTTP, RSS/Atom, HTML, special adapters, catalog, scheduler and quality rules.
- `src-tauri/src/browser.rs`: system Chrome/Chromium CDP lifecycle for rendered routes.
- `src-tauri/src/relevance.rs`: embedded policy evaluation and stable ruleset hash.
- `src-tauri/src/native.rs`: application orchestration, diagnostics, Excel and JSON output.
- `src-tauri/src/bin/news-scraper.rs`: headless command interface and Ctrl-C cancellation.
- `src-tauri/src/lib.rs`: Tauri commands, progress events and GUI cancellation.
- `src/`: Svelte GUI.

No scraper subprocess or language runtime is embedded. Network and browser failures retain route-level evidence; a successful fallback remains visible as an unstable source rather than being hidden.
