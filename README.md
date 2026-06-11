# News Scraper

中華民國政府政府部會每週新聞整合爬蟲，已由原本單檔程式拆分為可維護的套件結構。

目前支援 71 個政府機關與所屬單位，提供 Excel 匯出、資料品質檢查、來源健康監控、異常告警與可重現的 CI 測試。專案支援 Python 3.10、3.12 與 3.13。

完整的政府資料入口、抓取方式與技術參考請見 [資料來源與參考資料](SOURCES.md)。本專案只整理各機關公開發布的新聞與公告；內容著作權、正確性與最終解釋均以原發布機關網站為準。

## 目錄

```text
news_scraper/
  main.py                 CLI 入口
  config.py               URL、排序、timeout、headers
  models.py               資料模型
  excel_exporter.py       Excel 匯出
  http/                   requests / aiohttp 共用抓取
  rss/                    RSS / feedparser 共用解析
  utils/                  日期、文字、去重工具
  scrapers/
    registry.py           全來源註冊表
    base.py               scraper 共用 helper
    ministry/
      executive/          行政院
      oversight/          監察院
      finance/            財政金融與統計相關機關
      regulators/         委員會型機關
      foreign/            外交、陸委、僑委相關機關
      culture/            文化部與故宮
      digital/            數位發展部
      economy/            經濟部
      environment/        環境部
      development/        國發會、國科會及相關行政法人
      communities/        原民會與客委會
      sports/             運動部
      interior/           內政部及所屬機關
      justice/            法務部及所屬機關
      education/          教育部及相關機關
      transport/          交通部及所屬機關
      agriculture/        農業部及所屬機關
      health/             衛福部及所屬機關
      labor/              勞動部及所屬機關
      defense/            國防部及相關機關
      oceans/             海委會及所屬機關
      veterans/           退輔會及相關機關
```

## 安裝

```bash
python3 -m pip install -r requirements.txt
```

或：

```bash
python3 -m pip install -e .
```

若要建立與目前驗證環境相同的可重現環境：

```bash
python3 -m pip install -r requirements.lock.txt
```

開發與測試環境：

```bash
python3 -m pip install -e ".[dev]"
```

若需要固定開發工具版本：

```bash
python3 -m pip install -r requirements.lock.txt -r requirements-dev.lock.txt
```

完整本機品質檢查：

```bash
python3 -m ruff check .
python3 -m mypy
python3 -m compileall -q news_scraper
python3 -m pytest -q
```

> 若你的系統沒有 `python` 指令，請把下列指令的 `python` 改成 `python3`。

## 使用方式

列出目前支援來源：

```bash
python -m news_scraper --list-sources
```

抓取全部來源：

```bash
python -m news_scraper
```

未另外指定時，Excel 會預設輸出到專案根目錄的 `新聞搜集區/`。

只抓指定來源：

```bash
python -m news_scraper --sources 財政部 法務部 衛生福利部
```

指定輸出資料夾：

```bash
python -m news_scraper --output-dir /path/to/output
```

合併部會與所屬機關重複發布的同標題新聞：

```bash
python -m news_scraper --dedupe-affiliated
```

將 JSON 執行報告輸出到指定資料夾：

```bash
python -m news_scraper --report-dir /path/to/reports
```

預設會在 Excel 輸出資料夾下的 `執行紀錄/` 產生 JSON 報告，內容包含各來源每次嘗試的耗時、筆數、錯誤分類、最終失敗來源，以及實際使用不安全 SSL 降級的主機。

設定異常 webhook：

```bash
export NEWS_SCRAPER_ALERT_WEBHOOK="https://example.com/webhook"
python -m news_scraper
```

也可以用 `--alert-webhook` 單次指定。沒有 webhook 時，每週 automation 仍會透過既有 Gmail 流程寄送 JSON 異常摘要。以下狀況會傳送告警：來源最終失敗、達到來源專屬的連續零筆門檻、執行時間異常、無效資料，或重複／非新聞內容比例突然過高。

自動化或 CI 若需要在任何來源最終失敗時回傳非零結束碼：

```bash
python -m news_scraper --fail-on-source-error
```

國土管理署網站是 JavaScript 動態頁，且會出現一般 `curl`/HTTP2 不易直接讀取的防護頁；程式會改用 Selenium 開啟瀏覽器取得列表。若只抓國土管理署，建議確認本機已安裝 Chrome/Chromium，並已安裝 `requirements.txt` 內的 `selenium`：

```bash
python -m news_scraper --sources 國土管理署 --max-workers 1
```

## 輸出

程式會輸出：

- 終端機表格摘要
- `本週新聞整理（民國起迄日期）.xlsx`
- 全部新聞工作表
- AI 關鍵字初步篩選工作表
- 指定重點部會分頁

## 開發說明

- `SCRAPER_REGISTRY` 位於 `news_scraper/scrapers/registry.py`
- `SOURCE_ORDER` 位於 `news_scraper/config.py`
- 共用 RSS 規則優先放在 `rss/` 與 `scrapers/base.py`
- 可重用工具函式優先放在 `utils/`
- 下級機關 scraper 盡量放在對應部會子資料夾
- `monitoring.py` 負責錯誤分類與 JSON 執行報告
- `verify=False` 僅允許用於 `config.py` 的 SSL 白名單主機，所有實際降級主機都會記錄在執行報告

## 監控與告警

執行報告的 `status` 為 `success` 或 `partial_failure`。排程系統可監控最新報告中的下列欄位：

- `failed_sources`：重試後仍失敗的來源
- `error_counts`：`timeout`、`ssl`、`http`、`connection`、`browser`、`parse`、`unexpected` 分類統計
- `insecure_ssl_hosts`：本次實際停用 SSL 驗證的白名單主機
- `source_attempts`：各來源每次嘗試的耗時、結果與錯誤摘要
- `quality`：無效資料、重複新聞、排除的非新聞內容及各來源最終筆數
- `anomalies`：連續零筆與耗時異常
- `parser_warnings`：日期或欄位格式已命中但解析失敗的紀錄，用來區分正常零筆與網站格式異常
- `quality.alert_reasons`：超過告警門檻的資料品質問題；少量正常清理不會觸發告警

程式會先嘗試正常 SSL 驗證，只有驗證失敗且主機位於白名單時才降級。這讓已修復憑證的網站可以自動恢復安全連線，而不會永久停留在 `verify=False`。

執行報告預設保留 180 天，可用 `--report-retention-days` 調整。`執行紀錄/trend_summary.json` 會彙整最近 52 次報告的來源成功率、平均耗時與零筆次數。

累積至少 8 次報告後，`trend_summary.json` 的 `ssl_allowlist_audit.removal_candidates` 才會列出近期未曾使用 SSL 降級的白名單主機。移除前仍應以實際來源 smoke test 驗證，避免因低頻來源尚未執行而誤刪。

零筆告警依來源頻率分級：高頻來源通常連續 2 次、一般來源 4 次、低頻來源 8 次；農科園區因目前只出現徵才內容，暫時停用零筆告警。規則位於 `news_scraper/policy.toml`。

可用環境變數載入另一份政策檔，不必修改程式碼：

```bash
export NEWS_SCRAPER_POLICY_FILE=/path/to/policy.toml
python -m news_scraper
```

## 資料品質

匯出前會執行資料品質檢查：

- 排除缺少來源、日期、標題或網址的資料
- 排除無效日期與非 HTTP(S) 網址
- 移除追蹤參數與網址 fragment
- 移除同來源、同日期、同標題或同網址的重複資料
- 排除明確的事求人徵才內容

所有排除項目都會保留在 JSON 報告的 `quality.issues`，不會靜默遺失。

建議每次更新套件後重新執行完整測試，再以 `python3 -m pip freeze` 檢查並更新 `requirements.lock.txt`。

## 驗證

目前可用的基本檢查：

```bash
python3 -m compileall news_scraper
python -m news_scraper --list-sources
```

如果本機有安裝 `pytest`，也可以執行：

```bash
pytest -q
```

GitHub Actions 會在 Python 3.10、3.12 與 3.13 執行測試，並另外執行 Ruff 與 Mypy 品質檢查。

## 安全性與授權

本專案使用 [MIT License](LICENSE)。安全性問題請依照 [Security Policy](SECURITY.md) 私下回報，不要在公開 Issue 中張貼憑證、私人 webhook URL 或個人資料。
