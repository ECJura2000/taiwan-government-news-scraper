# 各機關新聞整理

[![Rust quality](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/test.yml)
[![Tauri v2](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml/badge.svg)](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/workflows/tauri-v2.yml)

v2.1.27 是完整 Rust 版；目前來源目錄含 88 個政府機關及主管財團法人來源。CLI、Tauri GUI、RSS／HTML／JSON、Chrome CDP、品質檢查、相關性規則、JSON schema v4 與 Excel 都由同一個 Rust application service 執行。新增法人及官方入口的查核依據見 [主管財團法人來源查核](docs/foundation-source-audit.md)。Excel 新聞日期使用斜線格式，新聞全文欄位會優先寫入官方完整內容；「開啟原文」欄會顯示「機關官網：完整網址」，並以同一網址建立可點擊的 Excel 超連結。經濟部改用官方 RSS 全文並保留瀏覽器列表頁備援；文策院採官方 Chrome CDP 路由，國防部僅在官方主機的 TLS 問題下使用瀏覽器備援。JSON 報告會記錄全文覆蓋率與摘要 fallback 數；入選新聞排序採關聯等級、規則分數、BM25、日期的穩定順序。v2.0.0 保留在 GitHub Releases 作為 rollback。

v2.1.27 重整桌面介面為「範圍確認 → 搜集新聞 → 查看報告」，採用標楷體與 Times New Roman，支援深淺主題與文字放大，詳見 [本版更新說明](docs/v2.1.27-release-notes.md)。

v2.1.26 加入 Excel／JSON 成對保存、全文品質提示、發布重試閘門、端到端回歸測試與設定載入檢查，並保留原有 Excel 深淺底色，詳見 [更新說明](docs/v2.1.26-release-notes.md)。

## 下載

從 [GitHub Releases](https://github.com/ECJura2000/taiwan-government-news-scraper/releases) 下載 `v2.1.27`，並先用 `SHA256SUMS.txt` 驗證。

- Windows 一般使用者：下載 `TaiwanGovernmentNews-Setup-v2.1.27.exe`。
- Windows 免安裝版：下載 `taiwan-government-news-v2.1.27-windows-portable.zip`，完整解壓後雙擊頂層的 `各機關新聞整理.exe`；進階 CLI 位於 `cli/news-scraper.exe`。
- macOS：下載 `macos-arm64`（Apple Silicon）或 `macos-x64`（Intel）ZIP；解壓縮後頂層會有 `各機關新聞整理.app`、`解除封鎖並開啟.command` 與 CLI `news-scraper`。
- Linux：下載對應平台 ZIP；CLI 在 ZIP 頂層，GUI installer 位於 `installers/`。

Windows 安裝檔會建立正常桌面應用入口，不需要開 CMD。macOS ZIP 的 `.app` bundle 經 `codesign --verify` 驗證；若 macOS 顯示「已損毀」或無法開啟，請先執行 ZIP 內的 `解除封鎖並開啟.command`，或在 Finder 對 `各機關新聞整理.app` 按右鍵後選「打開」。

封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。動態來源使用 Rust CDP 呼叫系統 Chrome／Chromium／Microsoft Edge；Windows 標準安裝位置會自動偵測。

GUI 與 Excel 採用同一套字體策略：中文內容使用標楷體，英文、數字、日期與規則 ID 使用 Times New Roman。GUI 執行百分比以完成來源比例推進至 90%，再依整理新聞、政策排序、Excel 與 JSON 寫入階段遞增至 99%，完成後才顯示 100%；單一來源內部下載不顯示假百分比。

## CLI

來源執行與 HTTP 傳輸政策的邊界、重構前基準及驗收方式見[來源與傳輸邊界](docs/SOURCE_TRANSPORT_BOUNDARIES.md)。

```bash
news-scraper list-sources
news-scraper collect
news-scraper collect --date 2026-08-06 --sources 財政部 法務部
news-scraper collect --start-date 2026-08-01 --end-date 2026-08-06
news-scraper collect --max-workers 8 --dedupe-affiliated
news-scraper collect --output-dir ./新聞搜集區 --report-dir ./新聞搜集區/執行紀錄
news-scraper collect --fail-on-source-error
news-scraper collect --content-mode summary
news-scraper collect --prefilter-mode shadow
```

預設 `--content-mode full` 沿用全文抓取、正式分類及 BM25。`summary` 收錄所有通過日期、新聞品質及去重規則的文章，只輸出來源、日期、標題、連結與列表摘要；不補摘要、不抓全文、不執行主題排除或分類。Excel 維持 17 欄，第六欄改標「列表摘要」，其餘分類／分數欄留空；檔名加 `_摘要`，避免覆蓋全文版本。JSON 的 `news_items` 保留逐篇列表摘要、route、detail_status 與 classification；摘要模式分類欄為 null，`evaluated=false`。

`--prefilter-mode shadow` 僅適用全文模式，仍抓取全部全文並產生正式結果。短於 80 字元或缺少摘要的文章列為 uncertain 並保留候選；政策名稱、核心／脈絡／加權詞及主管機關命中列為 candidate，其餘只作 rejected 預測。報告 `prefilter` 記錄規則雜湊、候選／排除數、逐篇高相關與可能相關漏判及提前去重差異。`detail_requests_avoided=0` 表示本期未真正節流；預估節省另記為 `simulated_detail_requests_avoided`。不提供 `on` 模式；summary 與 shadow 同用會在網路存取前拒絕。

`performance` 保存各階段耗時與漏斗筆數。來源階段時間為並行任務時間加總，不能當作整體 wall time 相加；整體來源處理時間為 `collection_wall_seconds`。`quality.mode_comparison` 在 full 執行用同一份 discovery 模擬摘要收錄結果，逐篇列出主題完全排除或去重所致差異；summary 沒有全文 control 時明確記為 unavailable。歷史來源品質基準按 content_mode 分開，舊報告視為 full。

正式條件式抓全文前須累積 3–5 個正常、可比週次，確認 shadow 高相關零漏判、可能相關漏判逐篇審查，並以同來源、日期與規則的控制組驗證總耗時至少下降 20%、請求量下降且來源失敗／fallback／429／parser regression 未惡化。Shadow 本身不會帶來全文請求節省；快取、並行度與正式節流留待後續階段。

未指定日期時，以 Asia/Taipei 當日計算：週一抓前一個完整週，其餘日期抓當週週一至週日。`--date` 使用指定日期所在週；它不能和 `--start-date/--end-date` 同時使用。

預設輸出：

- Excel：`新聞搜集區/本週新聞整理（民國起日至民國迄日）_執行編號.xlsx`（摘要模式另加 `_摘要`）
- JSON：`新聞搜集區/執行紀錄/news_scraper_run_*.json`

每次重跑保留獨立 Excel／JSON，不覆寫同週檔案。兩者寫完才建立 `.complete` 完成標記並更新 `latest_run.json`；摘要模式使用 `latest_summary_run.json`。排程應讀取對應模式的 latest 指標，核對完成標記、執行編號與 SHA-256，再依報告判讀是否可交付；完整協定見 [AI_AUTOMATION.md](docs/AI_AUTOMATION.md)。

全文模式會顯示全文取得數與覆蓋率；Excel 全文欄以 `【列表摘要；未取得全文】` 標示摘要補位，以 `【未取得全文或摘要】` 標示內容缺漏。摘要模式維持列表摘要，不補取全文。

判讀執行結果時必須同時查看 `status`、`failed_sources`、`anomalies`、`error_counts`、`quality.alert_reasons`、`source_health` 與 `relevance_policy.ruleset_hash`，不可只看 exit code。

v2.1.21 起另記錄各來源分類前篇數、內文補抓成功／失敗原因；連續三個可比週次後，來源量明顯下降會列入 `anomalies`。同站 HTTP 請求最多同時兩筆，遇到 429 會依 `Retry-After` 節制重試，列表頁支援本機 ETag／修改時間快取；超過 60 秒的等待要求會停止本次重試並保留診斷。Excel 仍維持原有 17 欄與十項政策主題。

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

來源 catalog 在 `src-tauri/resources/sources.json`；Rust adapters 在 `src-tauri/src/scraper/`；CDP 在 `src-tauri/src/browser.rs`；共用執行流程在 `src-tauri/src/native.rs`，Excel 與報告診斷分別位於 `src-tauri/src/native/excel.rs`、`src-tauri/src/native/report.rs`。

更多操作契約見 [AGENTS.md](AGENTS.md)、[AI 自動化](docs/AI_AUTOMATION.md) 與 [發布流程](docs/RELEASING.md)。

## 政策主題與介面設定

桌面首頁採「範圍確認 → 搜集新聞 → 查看報告」三步流程。新聞期間預設為臺北時區的上一完整週，也可指定週次或自訂起訖；CLI 的預設日期規則維持原樣。主題及來源設定按需展開，來源可按機關名稱搜尋，進階設定集中收合。保留淺色／深色及 100%–200% 字級，較矮視窗使用緊湊排版。

介面字型依序使用 Times New Roman 的英文／數字字形及標楷體中文字形；Windows 採 DFKai-SB，Mac 採 BiauKaiTC（標楷體-繁），並保留舊版 BiauKai 名稱。使用作業系統已安裝的字型，未將商用字型打包或提交至儲存庫；缺少指定字型時由系統 serif 字型替代。

開發介面預覽可使用 `npm run dev` 後的 `/?preview=1`；僅讀取公開來源目錄及預設主題，顯示預覽提示並停用搜集／資料夾選擇；原生檔案儲存操作需在桌面程式使用，不會模擬成功報告。正式桌面程式仍透過同一 Rust 引擎執行。

「搜尋主題」支援 JSON 匯入預覽、同名取代、清空後匯入、逐項刪除、啟停及規則編輯。目前內建十大主題的 201 個加權詞，保留政策來源頁碼及補充新聞詞的官網依據，首次啟動即可使用。請參閱 [主題 JSON 範例與格式](examples/topics/README.md)。

新聞依規則判斷相關性，再以中文斷詞及標題加權 BM25 排序。扣分詞及完全排除詞各自適用於所屬主題；甲排除、乙符合時仍可由乙收錄。各啟用主題產製獨立 Excel 工作表，JSON 報告保存實際設定雜湊、各主題筆數與排除統計。

介面採正體中文與中華民國法律用語，支援依系統、淺色及深色模式。Excel 來源欄為可複製的純文字，另由「開啟原文」欄開啟網址；日期維持西元預設，民國下拉選項為 `115-08-31` 格式。

政策詞逐項核對紀錄見 [政策詞來源核對](docs/policy-keywords-audit.md)。文字大小提供 100%、125%、150%、200%，與顯示模式分別保留。

本次測試與實際操作結果見 [第一階段驗證紀錄](docs/stage1-verification.md)。
