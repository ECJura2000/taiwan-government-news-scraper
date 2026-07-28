# Taiwan Government News Scraper Architecture

## Data Flow

```mermaid
flowchart LR
    A[Source catalog] --> B[Priority heap scheduler]
    B --> C[Source scrapers]
    C --> D[NewsItem domain model]
    D --> E[Validation and dedupe]
    E --> F[Stable source/date sort]
    F --> I[Versioned topic relevance scoring]
    I --> G[Excel exporter]
    I --> H[Run report JSON]
```

`source_catalog.py` validates the source configuration boundary. Scrapers return
`NewsItem` domain objects. JSON and Excel dictionaries are output DTOs created only at
the reporting/export boundary.

## Module Responsibilities

- `source_catalog.py`: validated source metadata and ordering.
- `scrapers/registry.py`: lazy loading of scraper callables.
- `scheduler.py`: heap-based execution priority.
- `models.py`: domain news model with optional summary and mapping compatibility.
- `quality.py`: validation, URL normalization, and duplicate removal.
- `monitoring.py`: typed attempts, parser warnings, report schema validation.
- `relevance.py`: validated topic profiles, fixed scoring, migration, and hashes.
- `relevance_editor.py`: GUI editing, import/export, filtering, and test classification.
- `excel_exporter.py`: presentation and workbook verification.
- `ai_policy_evaluation.py`: compatibility regression measurement for the built-in AI template.

## Data-Structure Choices And Complexity

| Structure / operation | Reason | Complexity |
| --- | --- | --- |
| `dict` source catalog | exact source lookup | average `O(1)` |
| `set` duplicate keys | one-pass duplicate detection | average `O(n)` time, `O(n)` space |
| `heap` source scheduler | deterministic risk-first priority | `O(n log n)` total |
| stable final sort | output independent from completion order | `O(n log n)` |
| typed dataclasses | constrain internal states and fields | constant-time field access |

The scheduler order and final Excel order are intentionally separate: execution order
optimizes latency, while final order optimizes reproducibility.

Each run loads one validated, immutable relevance-profile snapshot. Classification uses
title, optional summary, priority agencies, three include-keyword types, and scoped
exclusions to produce independent per-topic 0-100 scores plus auditable reasons. The
profile schema, template version, deterministic effective hash, and rule counts are written
to every run report and workbook. News records also retain date provenance, and quality
reports expose summary coverage and date-source counts.

## Error And Retry Policy

`errors.py` separates download, parse, validation, and storage failures. Only download
and underlying network/timeout failures enter the second scrape round. See
`docs/adr/0001-thread-pool-and-retry-policy.md`.

SSL fallback order is verified Requests, verified curl, then allowlisted `verify=False`.
Every insecure redirect target is checked against the allowlist before connecting.
Timezone-aware RSS timestamps are converted to `Asia/Taipei` before deriving calendar dates.
RSS collectors scan the complete returned feed rather than assuming strict reverse chronology.
