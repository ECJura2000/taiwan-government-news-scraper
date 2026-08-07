# AI and scheduler automation

Run from the repository root or use the published `news-scraper` executable:

```bash
cargo run --release --bin news-scraper -- collect
```

For a strict scheduled gate:

```bash
news-scraper collect --max-workers 8 --fail-on-source-error
```

After completion, inspect the newest workbook and JSON report. Report `status`, `news_count`, `failed_sources`, `failure_class_counts`, `anomalies`, `error_counts`, `source_health`, `quality.alert_reasons`, `relevance_policy.ruleset_hash`, `output_file` and `report_file`. Exit code 0 alone is insufficient evidence.

The optional `scripts/python_compat.py` bridge may be used only to forward the same command to an existing Rust executable. It does not provide a runtime or scraper implementation.
