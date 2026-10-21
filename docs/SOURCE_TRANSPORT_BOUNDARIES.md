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

來源的 `transport.list` 設定列表預設值；`routes[].transport` 可覆寫單一路由。`transport.detail[]` 依詳細內文網址的完整主機名稱選用政策。可設定 `timeout_seconds`、`retry_attempts`、`host_concurrency`、`cache`、`max_response_bytes`；route 可另外宣告 `tls_fallback_host`。瀏覽器可另宣告 `browser_native_user_agent`，由已解析的政策傳入 Chrome；預設保留既有相容識別資訊。目前只有逐站實測確認的種苗場列表啟用原生版本與平台資訊，不套用至其他來源或未宣告的詳細頁主機。

通用列表預設為 60 秒、3 次、同主機 2 筆、使用條件式快取、64 MiB；詳細內文相同但不快取。

目錄首次載入即檢查設定，不再把零值偷偷改成 1。逾時允許 1–3600 秒、嘗試次數 1–10、同主機並行 1–64、回應大小 1 byte–128 MiB；未知欄位、重複來源／route、未支援 kind／parser、無效 CSS selector 與非 HTTP(S)／含帳密 URL 都拒絕。詳細頁主機必須為不重複的完整小寫主機名稱。TLS 例外只接受國防部已宣告兩個 route 的 HTTPS `www.mnd.gov.tw`，不得放在來源預設或詳細頁政策。錯誤包含來源、route／主機及欄位，讓目錄修改能在 CI 與啟動時立即被發現。

勞動力發展署列表為 25 秒／1 次，國家資通安全研究院第一個 HTTP route 為 8 秒／1 次；法務部、國家公園署及農業部指定的詳細內文主機為 8 秒／1 次。國防部 TLS fallback 只能由其已宣告的兩個 route 使用，且主機必須精確為 `www.mnd.gov.tw`；不安全 TLS client 不跟隨跨站重導，成功使用仍記入 JSON 報告。瀏覽器 route 不經 HTTP client，保留系統 Chrome／Chromium 路由。

驗收時以固定 mock HTTP 案例檢查重試、429 `Retry-After`、快取、逾時、主機並行與大小限制；即時 smoke 的網路故障須和解析／輸出回歸分開記錄。來源驗收比對 `src-tauri/resources/approved_sources.json` 的完整名稱集合及數量；既有 88 個來源保留，新來源必須有官方關係、發布入口及解析樣本的驗證證據。

## 列表穩定性修正（2026-10-05）

- 國防部第一個 HTTP route 為 12 秒／1 次，避免首次連線拖滿通用 60 秒。解析相容 `headline-h4`／`headline-4` 與民國／西元日期，保留列表摘要；同來源的 `<base href>` 用於解析相對新聞連結，跨來源 base 不採用。列表存在卻無法解析日期與標題時，明確回報解析異常並嘗試備援。
- 客委會 HTTP route 為 8 秒／1 次，失敗後使用同一官方網址的系統瀏覽器 route；不擴大不安全 TLS 例外。詳細頁另以精確主機 `www.hakka.gov.tw` 宣告 `browser_fallback: true`，HTTP 失敗或解析不到內文時才補用 Chrome，並在 `source_diagnostics[].detail_fetch.browser_fallbacks` 保留原因與結果。其他主機不啟用此備援，摘要模式也不執行。HTTP 與瀏覽器頁面處理均採詳細頁政策的 8 秒期限；瀏覽器佇列及啟動時間另計，逾時後仍由擁有者清理程序與 profile。
- 公路局依序嘗試官方 HTML、列表所連結的官方 JSON、系統瀏覽器。兩個 HTTP route 均為 8 秒／1 次；JSON 最新日期早於查詢週時不接受為成功空集合，而改試下一路由。JSON 與瀏覽器備援的覆蓋仍受官方當下公布的列表範圍限制。
- 真正的 Incapsula 拒絕頁回報 `access_blocked`，不是 `parser_regression`。僅出現防護資源 script 不視為阻擋；瀏覽器遇到明確 `Request unsuccessful` 拒絕頁即停止，不再等 30 秒或重試該拒絕頁。不繞過 CAPTCHA 或存取控制。

以上改動不保證遠端網站永不拒絕連線。實測必須檢查 JSON 的各次 route、最終失敗來源與品質警示，不以程序 exit code 判定全數恢復。

國防部全文使用新版 `div.maincontent`；客委會全文使用 `div.d_sub div.sub_list div.editor`，避免把導覽列與頁尾當成新聞內文。列表成功不代表全文補取成功，應另外檢查 `detail_fetch.recovered`／`failed_or_empty`。

重構後於 2026-09-21 週以 Rust CLI 限定六個特殊來源（國防部、國家資通安全研究院、勞動力發展署、國家公園署、法務部、農業部）驗證：`status=success`、7 則新聞、無失敗來源／異常／品質警示，Excel 正常產生。國家資通安全研究院第一個 HTTP route 遇到 `runner_network`，官方瀏覽器 route 接續成功；`error_counts.connection=1`、`source_health.unstable_count=1`。國防部的受限 TLS fallback 使用記錄使 `ssl_fallback_host_count=1`。這些恢復紀錄不能誤述成「無錯誤」。

在 NPS 前綴解析移入來源元件後，再以國家公園署與國防部重跑同一固定週：兩次報告均為 `success`、2 則新聞、無失敗／異常／品質警示，來源嘗試的 route、篇數與分類相同（完成順序可因並行而變）。兩份 Excel 的第一工作表 XML 與 workbook XML SHA-256 相同；整個 ZIP 的位元組雜湊不同，不應據此誤判內容回歸。

公路局瀏覽器列表另宣告 `routes[].transport.browser_entry_url`，在同一次隔離 Chrome session 中先開官方首頁，再進入新聞列表，保留正常導覽 cookie。入口限同來源、無帳密 HTTP(S)，不新增 TLS 例外，也不跳過正式頁的驗證頁及解析檢查；其他路徑與詳細頁不繼承此設定。Cookie、profile 與完整週報不納入版本控制。

2026-10-13 的 GitHub 修正增加兩個可選的傳輸欄位。`tls_intermediate` 是固定官方 TWCA 中繼憑證與精確主機的政策；使用者明確核准消防署、農科園區、國健署三個主機，僅 Linux 使用，仍要求既有系統信任根、正確主機名稱及有效期限，不跟隨跨主機／HTTP 跳轉，沒有扩大國防部 invalid-certificate 例外。`accept_compression=false` 要求 identity 回應，目前只宣告在 WDA RSS，修復壓縮串流不正常結束；HTML 補足與詳細頁仍使用原預設。HTTP 用戶端只接受解析後的政策，不依政府網站名稱分支。查核與測試見 [GitHub 來源檢查修正](GITHUB_SOURCE_SMOKE_REPAIR.md)。

## 雲端失敗通知修正（2026-10-21）

環境部保留兩個原官方瀏覽器入口，新增第三順位的官方開放資料預覽 `MNEWS_P_01`，以 `transport.post_json` 宣告唯讀 JSON 查詢。設定僅允許 JSON route 的非空 object（上限 16 KiB），不得設於來源或詳細頁預設，也不得合用 TLS 例外。通用 HTTP client 依已解析政策送出 POST，沿用逾時、重試、同主機限制與大小上限；POST 不讀寫 GET 快取。

開放資料每三天更新一次，故此備援固定標記 `coverage_reduced=true`，不能當成即時完整新聞覆蓋。缺少新聞專頁網址時連到實際官方資料集頁，不從新聞編號猜網址；同一資料集連結下的不同標題／日期保留，重複標題仍去重。完整原文由官方資料記錄提供，不另依附件或宣傳連結猜文章網址。資料回應錯誤、空集合、缺日期／內容或快照不涵蓋要求週時明確失敗。

嘉南管理處將原 HTTP 的暫時網路錯誤重試由一次改為三次，並增加相同官方網址與選擇器的 Chrome 備援；來源數、分類條件及 TLS 政策不變。

## 列表日期與瀏覽器期限修正（2026-10-21）

最高檢察署改用正式新聞區 `section.lp .list`，從可見標題辨識民國／西元發布日期，保留原 PDF 連結，並依官方 `Page` 參數補足歷史列表。導覽列不再當成候選新聞；未標發布日期的協會活動紀錄不推測日期、也不收進週報；缺少正式列表、整頁沒有有效發布日期或新聞連結無效時明確回報解析異常，避免無日期資料在週篩選時全部消失。電子報標題中的月份不作為發布日，採其明載的發行日期。

最高檢察署與漁業署的 HTTP 列表為 12 秒／1 次，失敗後改試相同官方列表的 Chrome 備援（頁面處理 30 秒）。一般瀏覽器列表與後續分頁均套用已解析的 `transport.timeout_seconds`，與詳細頁一致；Chrome 佇列及啟動時間另計。逾時仍清理程序與暫存 profile。沒有文件網路失敗證據的 Chrome 逾時仍列為 `browser_runtime`，不偽裝成可豁免的網路故障，也不保證能解決遠端網站對雲端 IP 的拒絕連線。

漁業署 HTML 內文採官方 `.article-info dl.full > dd` 正文區，排除發布日期、點閱率、返回列表與頁尾；沒有正文的中繼資料頁仍回報空內文，不把整頁文字當作成功。
