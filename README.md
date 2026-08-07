# 各機關新聞整理

[![Rust quality](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml)
[![Tauri v2](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml)

v2.1.4 是完整 Rust 版：72 個政府來源、CLI、Tauri GUI、RSS／HTML／JSON、Chrome CDP、品質檢查、相關性規則、JSON schema v4 與 Excel 都由同一個 Rust application service 執行。v2.0.0 保留在 GitHub Releases 作為 rollback；v2.1.0、v2.1.1 與 v2.1.2 簽章候選版保留為未發布的稽核軌跡。

## 下載

從 [GitHub Releases](https://github.com/ECJura2000/taiwan-government-news-scraper/releases) 下載對應平台的 `v2.1.4` ZIP，並先用 `SHA256SUMS.txt` 驗證。每個 ZIP 內含：

- `news-scraper`／`news-scraper.exe`：headless CLI。
- `installers/`：Tauri GUI 安裝檔。
- `README.txt`：首次執行摘要。

Windows ZIP 另外含有 `START-GUI.cmd` 與 `TaiwanGovernmentNews-GUI.exe`。完整解壓後，直接雙擊 `START-GUI.cmd` 即可啟動圖形介面；若系統缺少 WebView2 或受到安全政策限制，請改用 `installers/` 內的 NSIS 安裝檔。

封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。動態來源使用 Rust CDP 呼叫系統 Chrome／Chromium。

## CLI

```bash
news-scraper list-sources
news-scraper collect
news-scraper collect --date 2026-08-06 --sources 財政部 法務部
news-scraper collect --start-date 2026-08-01 --end-date 2026-08-06
news-scraper collect --max-workers 8 --dedupe-affiliated
news-scraper collect --output-dir ./新聞搜集區 --report-dir ./新聞搜集區/執行紀錄
news-scraper collect --fail-on-source-error
```

未指定日期時，以 Asia/Taipei 當日計算：週一抓前一個完整週，其餘日期抓當週週一至週日。`--date` 使用指定日期所在週；它不能和 `--start-date/--end-date` 同時使用。

預設輸出：

- Excel：`新聞搜集區/本週新聞整理（民國起日 至 民國迄日）.xlsx`
- JSON：`新聞搜集區/執行紀錄/news_scraper_run_*.json`

判讀執行結果時必須同時查看 `status`、`failed_sources`、`anomalies`、`error_counts`、`quality.alert_reasons`、`source_health` 與 `relevance_policy.ruleset_hash`，不可只看 exit code。

## Python command bridge

唯一保留的 Python 檔案是 `scripts/python_compat.py`。它只把參數轉交給 Rust CLI，不 import scraper，也不執行任意 Python 程式：

```bash
python3 scripts/python_compat.py list-sources
python3 scripts/python_compat.py collect --date 2026-08-06 --sources 財政部 法務部
```

可用 `NEWS_SCRAPER_RUST_BIN` 指定 Rust executable。

## 原始碼建置與驗證

需求：stable Rust、Node.js 22、平台對應的 Tauri 2 系統函式庫；需要動態來源時另須 Chrome／Chromium。

```bash
npm ci
npm run check
npm run build
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --release --bin news-scraper
npm run tauri build
```

來源 catalog 在 `src-tauri/resources/sources.json`；Rust adapters 在 `src-tauri/src/scraper/`；CDP 在 `src-tauri/src/browser.rs`；共用 application service 與 Excel／JSON 匯出在 `src-tauri/src/native.rs`。

更多操作契約見 [AGENTS.md](AGENTS.md)、[AI 自動化](docs/AI_AUTOMATION.md) 與 [發布流程](docs/RELEASING.md)。
