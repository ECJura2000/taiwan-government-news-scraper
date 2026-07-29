# AI／Codex 自動化指南

這份說明適合要把新聞整理程式交給 Codex、AI agent、排程系統或其他自動化工具執行的人。

## 先選擇使用方式

| 需求 | 建議下載 |
| --- | --- |
| 只要 AI 啟動程式，不修改程式碼 | Release 頁面的 Windows、Linux 或 macOS 可攜版 ZIP |
| 希望 AI 閱讀、修改或新增來源 | 固定版本原始碼 ZIP |
| 開發者要持續取得更新 | 使用 Git clone |

原始碼下載入口：

- [v1.5.1 固定版本 AI 原始碼 ZIP](https://github.com/ECJura2000/taiwan-government-news-scraper/releases/download/v1.5.1/taiwan-government-news-v1.5.1-source.zip)
- [main 最新版本 ZIP](https://github.com/ECJura2000/taiwan-government-news-scraper/archive/refs/heads/main.zip)
- `git clone https://github.com/ECJura2000/taiwan-government-news-scraper.git`

正式排程建議鎖定版本標籤，不要直接使用會持續變動的 `main` ZIP。

## 建立原始碼環境

建議使用 Python 3.12。

macOS 或 Linux：

```bash
python3.12 -m venv .venv
.venv/bin/python -m pip install --require-hashes -r requirements.lock.txt
.venv/bin/python -m news_scraper --headless --json-summary
```

Windows PowerShell：

```powershell
py -3.12 -m venv .venv
.\.venv\Scripts\python.exe -m pip install --require-hashes -r requirements.lock.txt
.\.venv\Scripts\python.exe -m news_scraper --headless --json-summary
```

若環境沒有 Python 3.12，可使用專案支援的 Python 3.10 或 3.13。

從 v1.5.0 或更舊版本升級時，請重建 `.venv` 才能回收已移除的 pandas/numpy；更新鎖檔不會自動刪除舊環境中的套件。程式不會自行刪除 `.venv`。

## 交給 AI 的執行要求

可以直接把下列要求交給 AI 工具：

> 請在此專案根目錄執行每週新聞整理。優先使用專案內 `.venv` 的 Python，以 headless 模式執行並要求 JSON 摘要。完成後讀取最新 Excel 與最新 JSON 報告，明確列出 `status`、`failed_sources`、`anomalies`、`error_counts`、`quality.alert_reasons` 與 `relevance_policy.ruleset_hash`。不要只用結束碼判斷成功。

macOS／Linux 的標準指令：

```bash
.venv/bin/python -m news_scraper --headless --json-summary
```

Windows 的標準指令：

```powershell
.\.venv\Scripts\python.exe -m news_scraper --headless --json-summary
```

需要任一來源失敗時回傳非零結束碼，可加入：

```text
--fail-on-source-error
```

## 輸出與成功判定

- Excel 預設位於 `新聞搜集區/本週新聞整理*.xlsx`。
- JSON 報告預設位於 `新聞搜集區/執行紀錄/news_scraper_run_*.json`。
- 終端機最後一行的 JSON 摘要適合由 AI 或排程解析。
- 結束碼為 0 不代表所有外部來源都成功；仍須檢查 JSON 的 `status` 與 `failed_sources`。
- 少量正常去重或非新聞排除，且 `quality.alert_reasons` 為空時，不應誤判成程式失敗。
- GUI、headless 與排程應使用同一份 `程式資料/relevance-profile.json`。

## 自訂主題設定

要使用指定設定檔：

```bash
.venv/bin/python -m news_scraper \
  --headless \
  --json-summary \
  --relevance-profile /path/to/relevance-profile.json
```

若要忽略共用設定並使用內建範本：

```bash
.venv/bin/python -m news_scraper --headless --json-summary --no-relevance-profile
```

## 安全事項

- 不要把 Gmail 密碼、webhook URL、API key 或其他憑證寫入專案或主題設定檔。
- 國土管理署來源需要系統已安裝 Chrome 或 Chromium。
- AI 修改來源解析器後，至少執行 `python -m pytest -q` 與 `python -m news_scraper --list-sources`。
- 自動寄信時，必須取得郵件服務明確回傳的 message ID 或成功狀態，才能宣告寄送成功。
