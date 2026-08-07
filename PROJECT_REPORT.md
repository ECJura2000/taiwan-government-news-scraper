# v2.1.4 Project Report

v2.1.4 replaces the previous scraper runtime with Rust across CLI, Tauri, 72 source routes, CDP, quality diagnostics, relevance rules, Excel and schema-v4 JSON. It includes a directly launchable Tauri executable in the Windows portable ZIP and verifies that both build-time and published-archive GUI processes remain alive after startup. It also preserves the Windows CLI and NSIS installer, complete source retry evidence, and isolated release bundles.

Acceptance evidence includes 72/72 live route completion, fixed-fixture field parity, exact selected-run parity across all 15 Excel fields, a stable relevance hash (`e75be08ee1c5dab8`), cancellation-without-partial-artifact testing, frontend checks, Rust unit/integration tests and release workflows for Linux, Windows, macOS Apple Silicon and macOS Intel.

GitHub publication remains gated on all-source health, four-platform bundles, dependency audits, archive-content checks, checksums, size manifests and post-release smoke. v2.0.0 remains immutable as rollback.
