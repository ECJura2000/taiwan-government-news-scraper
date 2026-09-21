# 各機關新聞整理

[![Rust quality](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml)
[![Tauri v2](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml)

v2.1.15 是完整 Rust 版：72 個政府來源、CLI、Tauri GUI、RSS／HTML／JSON、Chrome CDP、品質檢查、相關性規則、JSON schema v4 與 Excel 都由同一個 Rust application service 執行。Excel 新聞日期使用斜線格式，新聞全文欄位會優先寫入官方完整內容；經濟部改用官方 RSS 全文並保留瀏覽器列表頁備援。JSON 報告會記錄全文覆蓋率與摘要 fallback 數；入選新聞排序採關聯等級、規則分數、BM25、日期的穩定順序。v2.0.0 保留在 GitHub Releases 作為 rollback。

## 下載

從 [GitHub Releases](https://github.com/ECJura2000/taiwan-government-news-scraper/releases) 下載 `v2.1.15`，並先用 `SHA256SUMS.txt` 驗證。

- Windows 一般使用者：下載 `TaiwanGovernmentNews-Setup-v2.1.15.exe`。
- Windows 免安裝版：下載 `taiwan-government-news-v2.1.15-windows-portable.zip`，完整解壓後雙擊頂層的 `各機關新聞整理.exe`；進階 CLI 位於 `cli/news-scraper.exe`。
- macOS：下載 `macos-arm64`（Apple Silicon）或 `macos-x64`（Intel）ZIP；解壓縮後頂層會有 `各機關新聞整理.app`、`解除封鎖並開啟.command` 與 CLI `news-scraper`。
- Linux：下載對應平台 ZIP；CLI 在 ZIP 頂層，GUI installer 位於 `installers/`。

Windows 安裝檔會建立正常桌面應用入口，不需要開 CMD。macOS ZIP 的 `.app` bundle 經 `codesign --verify` 驗證；若 macOS 顯示「已損毀」或無法開啟，請先執行 ZIP 內的 `解除封鎖並開啟.command`，或在 Finder 對 `各機關新聞整理.app` 按右鍵後選「打開」。

封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。動態來源使用 Rust CDP 呼叫系統 Chrome／Chromium／Microsoft Edge；Windows 標準安裝位置會自動偵測。

GUI 與 Excel 採用同一套字體策略：中文內容使用標楷體，英文、數字、日期與規則 ID 使用 Times New Roman。GUI 執行百分比以完成來源比例推進至 90%，再依整理新聞、政策排序、Excel 與 JSON 寫入階段遞增至 99%，完成後才顯示 100%；單一來源內部下載不顯示假百分比。

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
npm test
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

## 政策主題與介面設定

「搜尋主題」支援 JSON 匯入預覽、同名取代、清空後匯入、逐項刪除、啟停及規則編輯。製作階段已完成十大主題的 173 個加權詞與政策來源頁碼，首次啟動即可使用。請參閱 [主題 JSON 範例與格式](examples/topics/README.md)。

新聞依規則判斷相關性，再以中文斷詞及標題加權 BM25 排序。扣分詞及完全排除詞各自適用於所屬主題；甲排除、乙符合時仍可由乙收錄。各啟用主題產製獨立 Excel 工作表，JSON 報告保存實際設定雜湊、各主題筆數與排除統計。

介面採正體中文與中華民國法律用語，支援依系統、淺色及深色模式。Excel 來源欄為可複製的純文字，另由「開啟原文」欄開啟網址；日期維持西元預設，民國下拉選項為 `115-08-31` 格式。

政策詞逐項核對紀錄見 [政策詞來源核對](docs/policy-keywords-audit.md)。文字大小提供 100%、125%、150%、200%，與顯示模式分別保留。

本次測試與實際操作結果見 [第一階段驗證紀錄](docs/stage1-verification.md)。
