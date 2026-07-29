# AI Agent Execution Contract

Use this file when an AI coding or automation agent opens the repository.

## Runtime

- Work from the repository root.
- Prefer `.venv/bin/python` on macOS/Linux.
- Prefer `.venv\Scripts\python.exe` on Windows.
- If `.venv` does not exist, create it with Python 3.12 and install `requirements.lock.txt` with `--require-hashes`.
- Use headless mode for automation. Do not open the GUI unless the user asks for it.

## Standard Run

macOS/Linux:

```bash
.venv/bin/python -m news_scraper --headless --json-summary
```

Windows:

```powershell
.\.venv\Scripts\python.exe -m news_scraper --headless --json-summary
```

## Result Contract

- Excel: newest `新聞搜集區/本週新聞整理*.xlsx`
- Report: newest `新聞搜集區/執行紀錄/news_scraper_run_*.json`
- Inspect `status`, `failed_sources`, `anomalies`, `error_counts`, `quality.alert_reasons`, and `relevance_policy.ruleset_hash`.
- Do not claim full success from exit code 0 alone.
- Normal deduplication or non-news filtering is not a failure when `quality.alert_reasons` is empty.
- Use `--fail-on-source-error` only when the caller requires a nonzero exit code for any failed source.

## Change Validation

Run these checks after modifying code:

```bash
.venv/bin/python -m ruff check .
.venv/bin/python -m mypy
.venv/bin/python -m pytest -q
.venv/bin/python -m news_scraper --list-sources
```

Do not commit generated Excel files, JSON run reports, credentials, or `程式資料/relevance-profile.json`.
