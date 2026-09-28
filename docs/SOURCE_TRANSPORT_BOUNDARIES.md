# 來源執行與傳輸邊界

## 重構前基準

基準提交為 v2.1.22（`78d2902`），預設目錄為 88 個來源。重構前 `cargo test --workspace --all-targets` 通過：函式庫 80 個、CLI 3 個、整合測試 15 個（另有 1 個需要即時 RSS 的測試略過）；`list-sources` 列出 88 個名稱。既有固定案例涵蓋 route 嘗試與失敗分類、來源健康、詳細內文失敗、17 欄 Excel 及 JSON schema v4。

2026-09-21 週的重構前完整本機報告：88 個來源、375 則新聞、`partial_failure`（監察院與勞動力發展署屬 `source_outage`）、`anomalies=[]`、`quality.alert_reasons=[]`。這是網站當時的狀態，不應拿即時新聞數量當程式回歸的唯一判準。產出的 Excel 與 JSON 不納入版本控制。

本機 HTTP 快取樣本中最大檔案約 1.9 MB。新設的預設回應上限為 64 MiB，遠高於已觀察樣本；超限會明確失敗，不會回傳截斷文字。國家公園署近期 JSON 前綴仍由來源執行元件處理，並保留完整資料回補判斷。

## 職責

- `native.rs`：選取來源、排程、收集結果、品質處理、政策排序與輸出協調。
- `native/source.rs`：來源 route 嘗試、瀏覽器路由、NPS 近期 JSON 前綴、日期篩選、詳細內文補取及來源診斷。
- `native/report.rs`、`native/excel.rs`：維持 JSON 報告與 Excel 輸出的既有職責。
- `scraper/transport.rs`：從來源目錄解析列表 route 與詳細內文政策；未設定時使用通用預設值。
- `scraper/http.rs`：只接收已解析的政策，執行逾時、重試、主機並行限制、ETag/Last-Modified 快取、回應大小限制及受限的 TLS fallback；不判斷來源或政府網站名稱。

## 來源政策契約

來源的 `transport.list` 設定列表預設值；`routes[].transport` 可覆寫單一路由。`transport.detail[]` 依詳細內文網址的完整主機名稱選用政策。可設定 `timeout_seconds`、`retry_attempts`、`host_concurrency`、`cache`、`max_response_bytes`；route 可另外宣告 `tls_fallback_host`。通用列表預設為 60 秒、3 次、同主機 2 筆、使用條件式快取、64 MiB；詳細內文相同但不快取。

勞動力發展署列表為 25 秒／1 次，國家資通安全研究院第一個 HTTP route 為 8 秒／1 次；法務部、國家公園署及農業部指定的詳細內文主機為 8 秒／1 次。國防部 TLS fallback 只能由其已宣告的兩個 route 使用，且主機必須精確為 `www.mnd.gov.tw`；不安全 TLS client 不跟隨跨站重導，成功使用仍記入 JSON 報告。瀏覽器 route 不經 HTTP client，保留系統 Chrome／Chromium 路由。

驗收時以固定 mock HTTP 案例檢查重試、429 `Retry-After`、快取、逾時、主機並行與大小限制；即時 smoke 的網路故障須和解析／輸出回歸分開記錄。來源數驗收依現行核准目錄為 **88**，不是舊計畫的 73。

重構後於 2026-09-21 週以 Rust CLI 限定六個特殊來源（國防部、國家資通安全研究院、勞動力發展署、國家公園署、法務部、農業部）驗證：`status=success`、7 則新聞、無失敗來源／異常／品質警示，Excel 正常產生。國家資通安全研究院第一個 HTTP route 遇到 `runner_network`，官方瀏覽器 route 接續成功；`error_counts.connection=1`、`source_health.unstable_count=1`。國防部的受限 TLS fallback 使用記錄使 `ssl_fallback_host_count=1`。這些恢復紀錄不能誤述成「無錯誤」。

在 NPS 前綴解析移入來源元件後，再以國家公園署與國防部重跑同一固定週：兩次報告均為 `success`、2 則新聞、無失敗／異常／品質警示，來源嘗試的 route、篇數與分類相同（完成順序可因並行而變）。兩份 Excel 的第一工作表 XML 與 workbook XML SHA-256 相同；整個 ZIP 的位元組雜湊不同，不應據此誤判內容回歸。
