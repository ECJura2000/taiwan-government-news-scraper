# Releasing v2.1.4

## Local gate

```bash
npm ci
npm run check
npm run build
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
npm audit --audit-level=high
cargo run --release --bin news-scraper -- collect --date 2026-08-06 --fail-on-source-error
npm run tauri build
```

Inspect the JSON contract and Excel workbook. Confirm 72 selected sources, no failed source, no quality alert, schema version 4 and the expected relevance hash.

## Publication gate

1. Merge the validated commit to `main`.
2. Create and push immutable tag `v2.1.4`; do not modify `v2.0.0`, the failed unpublished `v2.1.0`, `v2.1.1`, `v2.1.2` candidates, or the published `v2.1.3` release.
3. The `Build and release Rust apps` workflow must pass the all-source gate and all four platform builds.
4. Verify four ZIPs, SHA-256 manifest, size manifest and the non-draft GitHub Release.
5. Confirm archives contain the Rust CLI and Tauri installer but no `.py`, Python runtime, PyInstaller, openpyxl or Selenium.
6. Run `Release live smoke` for `v2.1.4` and verify all four published downloads, including the extracted Windows portable GUI process smoke.

A tag without a completed workflow, release assets and successful smoke is not a completed release.
