# v2.1.3 Project Report

v2.1.3 replaces the previous scraper runtime with Rust across CLI, Tauri, 72 source routes, CDP, quality diagnostics, relevance rules, Excel and schema-v4 JSON. It serializes CDP startup on low-resource runners, adds a browser fallback for the Ministry of Culture, preserves complete retry evidence without treating classified external availability failures as code regressions, and isolates release bundles from source-health artifacts.

Acceptance evidence includes 72/72 live route completion, fixed-fixture field parity, exact selected-run parity across all 15 Excel fields, a stable relevance hash (`e75be08ee1c5dab8`), cancellation-without-partial-artifact testing, frontend checks, Rust unit/integration tests and release workflows for Linux, Windows, macOS Apple Silicon and macOS Intel.

GitHub publication remains gated on all-source health, four-platform bundles, dependency audits, archive-content checks, checksums, size manifests and post-release smoke. v2.0.0 remains immutable as rollback.
