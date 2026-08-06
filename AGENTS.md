# AI Agent Execution Contract

## Runtime

- Work from the repository root.
- Use the Rust CLI for headless work. Do not open the Tauri GUI unless requested.
- Dynamic routes require a system Chrome or Chromium executable.

## Standard Run

```bash
cargo run --release --bin news-scraper -- collect
```

Use `--date YYYY-MM-DD` for a deterministic week and `--fail-on-source-error` only when any failed source must produce a nonzero exit code.

## Result Contract

- Excel: newest `新聞搜集區/本週新聞整理*.xlsx`.
- Report: newest `新聞搜集區/執行紀錄/news_scraper_run_*.json`.
- Inspect `status`, `failed_sources`, `anomalies`, `error_counts`, `quality.alert_reasons`, `source_health`, and `relevance_policy.ruleset_hash`.
- Do not claim full success from exit code 0 alone.
- Normal deduplication or non-news filtering is not a failure when `quality.alert_reasons` is empty.

## Change Validation

```bash
npm run check
npm run build
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run --quiet --bin news-scraper -- list-sources
```

The source count must remain 72. Do not commit generated Excel files, JSON run reports, credentials, browser profiles, `target/`, or `node_modules/`.

Only `scripts/python_compat.py` may be a Python file. It is a command bridge to the Rust executable and must not contain scraper logic or import another project module.
