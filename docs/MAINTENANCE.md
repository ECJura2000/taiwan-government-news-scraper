# Maintenance

For every source change:

1. Update the declarative route or the smallest relevant Rust adapter.
2. Add a fixture or parser regression test.
3. Run formatting, all Rust tests, clippy, Svelte checks and the 88-source catalog assertion.
4. Run the affected source live with a fixed `--date` and inspect JSON diagnostics plus workbook fields.
5. Run all sources before release; retain fallback and quality warnings in evidence.

Dependency changes require `cargo audit`, `npm audit`, local build verification and the four-platform GitHub matrix. Never commit generated reports, browser profiles, credentials or local relevance profiles.

The weekly report compares each successful source's pre-filter scrape count with at least three distinct, completed historical weeks using the same relevance ruleset. The new baseline begins with reports containing `quality.source_scraped_counts`; older reports cannot establish it retroactively. Low volume is an attention signal, not a source failure; a source with no established publishing baseline may legitimately return zero items. Sources with five or more detail fetches and zero recovery appear in `quality.detail_fetch_zero_recovery_sources`; they become alerts only when three comparable historical weeks establish a high full-text coverage baseline. This distinguishes longstanding parser coverage debt from a new regression. Check `anomalies`, `quality.alert_reasons`, `source_diagnostics[].detail_fetch`, and the full-text coverage fields before declaring a collection complete. HTTP list responses may be conditionally revalidated using the local, ignored `新聞搜集區/.http-cache/` directory; detail pages are never served from this cache. Requests are capped at two concurrent requests per host, and a `Retry-After` longer than 60 seconds stops retries for that source rather than retrying before the site's requested time.
