# Releasing v2.1.26

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
cargo run --release --bin news-scraper -- collect --date 2026-09-21
npm run tauri build
```

Inspect the JSON contract and Excel workbook. Confirm 88 selected sources, classify any failed source by cause, require no quality alert, schema version 4 and the expected relevance hash. A temporary external outage must be documented and must not be described as a full success.

## Publication gate

1. Merge the validated commit to `main`.
2. Create and push immutable tag `v2.1.26`; do not modify any previous tag or Release, including the published rollback `v2.0.0`.
3. The `Build and release Rust apps` workflow must pass the all-source gate and all four platform builds.
4. Verify the Linux/macOS ZIPs, Windows portable ZIP, Windows Setup EXE, SHA-256 manifest, size manifest and the non-draft GitHub Release.
5. Confirm archives contain the Rust CLI and Tauri app but no `.py`, Python runtime, PyInstaller, openpyxl or Selenium.
6. Confirm the Windows portable ZIP has top-level `各機關新聞整理.exe`, `cli/news-scraper.exe`, and no `START-GUI.cmd`.
7. Run `Release live smoke` for `v2.1.26` and verify all published downloads, including the extracted Windows portable GUI rendered-interface marker.

A tag without a completed workflow, release assets and successful smoke is not a completed release.

## Source-health retry policy

The full 88-source run must satisfy schema, policy-hash, nonempty-news and quality-alert checks. Each initially failed source receives one separate CLI retry before the gate evaluates its final failure class. A recovered parser failure is accepted with both attempts retained; unresolved parser/access/runtime failures block publication. Remaining failures may only be attributed to `source_outage`, `runner_network` or `tls_certificate`, with valid nonzero counts and matching retry evidence.

The default ceiling is **8 final failed sources and 10%**, both enforced. 數位發展部、國發會、國科會 and 經濟部 must recover regardless of that ceiling. The explicit policy lives in `scripts/release-source-gate.mjs`; update its tested defaults deliberately when the release policy changes. `source-health/final-gate.json` records the initial/final failure lists, policy limits and rejection reasons, alongside all original reports and retry evidence. Do not describe an accepted degraded run as all sources successful.
