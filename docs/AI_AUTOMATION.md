# AI and scheduler automation

Run from the repository root or use the published `news-scraper` executable:

```bash
cargo run --release --bin news-scraper -- collect
```

For a strict scheduled gate:

```bash
news-scraper collect --max-workers 8 --fail-on-source-error
```

After completion, use the CLI summary's `output_file` and `report_file`, or read `執行紀錄/latest_run.json` (`latest_summary_run.json` for summary mode). Report `status`, `news_count`, `failed_sources`, `failure_class_counts`, `anomalies`, `error_counts`, `source_health`, `quality.alert_reasons`, `relevance_policy.ruleset_hash`, `output_file` and `report_file`. Exit code 0 alone is insufficient evidence.

Each run has a common timestamp ID in the Excel/JSON filenames and the workbook version sheet. Files are staged in temporary directories on their respective destination filesystems. Publication never replaces an existing run. Only after both files are complete does the writer publish `news_scraper_run_<id>.complete` and atomically replace the mode's latest pointer. Both manifests contain `run_id`, `output_file`, `report_file`, `excel_sha256` and `report_sha256`.

Before delivery, read the pointer once, verify its matching `.complete` manifest, compare the report's `run_id` and `output_file`, and check both SHA-256 values. A completed pair may still report `partial_failure` or `attention`; completion means the pair is saved, not that collection succeeded. Ordinary publication failure rolls back files owned by that run and preserves previous results. An abrupt process/OS shutdown can leave uncommitted files; ignore any new-format report lacking its completion marker. Historical reports without `run_id` remain readable. Output storage must support hard links and atomic file replacement; unsupported storage fails rather than overwriting an older run.

Schema version remains 4; new fields are additive. `news_items` now includes `department`, `category`, `summary`, `full_text` and `content_status` (`full_text`, `summary_only`, `missing`). They preserve raw collected content; Excel alone labels summary substitution. Check `quality.full_text_count`, `full_text_coverage_rate`, `description_fallback_count`, `detail_fetch_attempted_count`, `detail_fetch_failed_or_empty_count` and `content_warnings` separately from source health. Full mode emits `low_full_text_coverage` when at least 10 final items have less than 50% full-text coverage. This informational warning does not become a source failure or a quality alert. Summary mode deliberately makes no detail requests and does not emit that warning.

Contract regressions run with `cargo test --workspace --all-targets`: real CLI output/exit behavior and paired artifact hashes, rollback and mode pointers, and parser → JSON → Excel cell/link semantics. Release gate cases run with `npm test`.

The optional `scripts/python_compat.py` bridge may be used only to forward the same command to an existing Rust executable. It does not provide a runtime or scraper implementation.
