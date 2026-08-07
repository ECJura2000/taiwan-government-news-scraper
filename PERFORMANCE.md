# Performance

The Rust service uses bounded asynchronous source concurrency (`--max-workers`, default 8). Output ordering remains deterministic after concurrent collection. Deduplication and quality validation are linear in record count; final workbook ordering is O(n log n).

The 2026-08-03 through 2026-08-09 live acceptance run covered all 72 sources and completed in about 75 seconds on macOS with 298 valid records before the final RSS-link correction. Performance conclusions should always record hardware, date range, source health and network conditions.

Use a release build for measurements:

```bash
/usr/bin/time -p cargo run --release --bin news-scraper -- collect --date 2026-08-06 --max-workers 8
```
