# 各機關新聞整理

[![Rust quality](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml)
[![Tauri v2](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml)

v2.1.9 是完整 Rust 版：72 個政府來源、CLI、Tauri GUI、RSS／HTML／JSON、Chrome CDP、品質檢查、相關性規則、JSON schema v4 與 Excel 都由同一個 Rust application service 執行。此版修正 Excel 與 GUI 字體，讓中文以標楷體呈現、英文與數字保留 Times New Roman，並調整 GUI 執行進度計算，避免啟動後長時間停在 0%。v2.0.0 保留在 GitHub Releases 作為 rollback。

## 下載

從 [GitHub Releases](https://github.com/ECJura2000/taiwan-government-news-scraper/releases) 下載 `v2.1.9`，並先用 `SHA256SUMS.txt` 驗證。

- Windows 一般使用者：下載 `TaiwanGovernmentNews-Setup-v2.1.9.exe`。
- Windows 免安裝版：下載 `taiwan-government-news-v2.1.9-windows-portable.zip`，完整解壓後雙擊頂層的 `各機關新聞整理.exe`；進階 CLI 位於 `cli/news-scraper.exe`。
- macOS：下載 `macos-arm64`（Apple Silicon）或 `macos-x64`（Intel）ZIP；解壓縮後頂層會有 `各機關新聞整理.app`、`解除封鎖並開啟.command` 與 CLI `news-scraper`。
- Linux：下載對應平台 ZIP；CLI 在 ZIP 頂層，GUI installer 位於 `installers/`。

Windows 安裝檔會建立正常桌面應用入口，不需要開 CMD。macOS ZIP 的 `.app` bundle 經 `codesign --verify` 驗證；若 macOS 顯示「已損毀」或無法開啟，請先執行 ZIP 內的 `解除封鎖並開啟.command`，或在 Finder 對 `各機關新聞整理.app` 按右鍵後選「打開」。

封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。動態來源使用 Rust CDP 呼叫系統 Chrome／Chromium／Microsoft Edge；Windows 標準安裝位置會自動偵測。

GUI 與 Excel 採用同一套字體策略：中文內容使用標楷體，英文、數字、日期與規則 ID 使用 Times New Roman。GUI 執行百分比以準備階段 5%、來源完成比例 90%、寫出 Excel/JSON 95%、完成 100% 顯示；單一來源內部下載不顯示假百分比。

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

需求：stable Rust、Node.js 22、平台對應的 Tauri 2 系統函式庫；需要動態來源時另須 Chrome、Chromium 或 Microsoft Edge。

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
