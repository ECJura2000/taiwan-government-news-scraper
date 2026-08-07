# Maintenance

For every source change:

1. Update the declarative route or the smallest relevant Rust adapter.
2. Add a fixture or parser regression test.
3. Run formatting, all Rust tests, clippy, Svelte checks and the 72-source catalog assertion.
4. Run the affected source live with a fixed `--date` and inspect JSON diagnostics plus workbook fields.
5. Run all sources before release; retain fallback and quality warnings in evidence.

Dependency changes require `cargo audit`, `npm audit`, local build verification and the four-platform GitHub matrix. Never commit generated reports, browser profiles, credentials or local relevance profiles.
