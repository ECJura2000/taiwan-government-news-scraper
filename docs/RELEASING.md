# Releasing v2.1.11

## Local gate

```bash
npm ci
npm test
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
2. Create and push immutable tag `v2.1.11`; do not modify any previous tag or Release, including the published rollback `v2.0.0`.
3. The `Build and release Rust apps` workflow must pass the all-source gate and all four platform builds.
4. Verify the Linux/macOS ZIPs, Windows portable ZIP, Windows Setup EXE, SHA-256 manifest, size manifest and the non-draft GitHub Release.
5. Confirm archives contain the Rust CLI and Tauri app but no `.py`, Python runtime, PyInstaller, openpyxl or Selenium.
6. Confirm the Windows portable ZIP has top-level `各機關新聞整理.exe`, `cli/news-scraper.exe`, and no `START-GUI.cmd`.
7. Run `Release live smoke` for `v2.1.11` and verify all published downloads, including the extracted Windows portable GUI rendered-interface marker.

A tag without a completed workflow, release assets and successful smoke is not a completed release.
