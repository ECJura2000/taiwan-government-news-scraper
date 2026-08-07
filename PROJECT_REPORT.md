# v2.1.5 Project Report

v2.1.5 replaces the previous scraper runtime with Rust across CLI, Tauri, 72 source routes, CDP, quality diagnostics, relevance rules, Excel and schema-v4 JSON. It fixes the blank GUI caused by constructing a Svelte 5 component with the removed legacy class API, uses the supported `mount()` API, and requires Windows build and published-archive smoke tests to observe the rendered-interface ready title rather than merely a live process.

Acceptance evidence includes 72/72 live route completion, fixed-fixture field parity, exact selected-run parity across all 15 Excel fields, a stable relevance hash (`e75be08ee1c5dab8`), cancellation-without-partial-artifact testing, frontend checks, Rust unit/integration tests and release workflows for Linux, Windows, macOS Apple Silicon and macOS Intel.

GitHub publication remains gated on all-source health, four-platform bundles, dependency audits, archive-content checks, checksums, size manifests and post-release smoke. v2.0.0 remains immutable as rollback.
