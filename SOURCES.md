# Sources

The authoritative source list is the embedded declarative catalog at `src-tauri/resources/sources.json`. It contains 72 named official sources, their primary URLs, explicit fallback routes, parser types, priority and reduced-coverage metadata.

Source adapters live in `src-tauri/src/scraper/`:

- RSS and Atom: `rss.rs`
- generic and profiled HTML: `html.rs` and `adapters.rs`
- JSON and special official formats: `special.rs`
- dynamic official pages: Rust CDP through `browser.rs`

Run `news-scraper list-sources` to print the effective ordered catalog. Every catalog entry is covered by tests requiring a non-empty official URL and effective route metadata. Source changes must preserve route diagnostics and must not silently turn a parser regression into a successful zero-row result.
