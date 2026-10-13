# GitHub 來源檢查修正

## 問題與證據

2026-10-13 原始排程 [38339928715](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/runs/38339928715) 全量 192 個來源取得 87 則，29 個來源重測仍失敗。17 個被粗略分類為網路失敗，9 個為 Chrome 導航逾時、文化部為驗證頁阻擋，其餘兩個為網站／回應故障；這次工作流程確實沒有通過。

[Linux 網路獨立診斷](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/runs/38370389589) 使用啟用憑證驗證的 curl，記錄 DNS、TCP、TLS、HTTP 階段。大多數農業來源在雲端 TCP 連線尚未建立就逾時；沒有證據可把它們當成解析成功。消防署、農科園區、國健署的官方鏈缺中繼，Linux OpenSSL 明確拒絕。其他網站沒有盲目套用補鏈或放寬 TLS。

Mac 2026-10-05 週全量 `20261013_164019_513440` 取得 528 則，原始狀態 `partial_failure`（5 個），沒有異常或品質警示，JSON／Excel 雜湊一致，Excel 17 欄、529 列。23 個雲端受限來源在此全量檢查均成功，其逐來源主機、證據連結及 Mac run ID 記錄於 `src-tauri/resources/source_verification_profiles.json`。這份查核名單不是生成的執行報告，也沒有移除來源。

## 修正

- HTTP 保留底層 DNS／TCP／TLS 原因鏈；不再只留下 `error sending request`。
- Chrome 觀察主文件的 `Page.navigate.errorText`、`Network.loadingFailed` 及指定 loader 載入事件；不把圖片／其他 iframe 失敗當成新聞失敗，也不吞掉早於導航回覆到達的載入事件。沒有具體網路證據的逾時仍是瀏覽器錯誤。
- 國防院非標準 HTTP 標頭、故宮直接請求 403：保留原路徑，增加同一官方列表的 Chrome 備援。Mac 單次兩來源 `20261013_165826_777348` 為 `success`、4 則，原始列表分別 16、20 筆，原始錯誤仍保留。
- 使用者明確核准的三主機 Linux 中繼補鏈：官方 DER 指紋固定，僅宣告主機可選用，不跨主機、不降為 HTTP，不套用 macOS／Windows。Linux [實際 TLS 測試](https://github.com/ECJura2000/taiwan-government-news-scraper/actions/runs/38370389780) 證明缺根、缺中繼與主機不符仍拒絕，完整鏈通過，另檢查過期鏈拒絕。沒有增加國防部以外的 invalid-certificate fallback。
- 日常預設 cloud 範圍 169 個，23 個須由 Mac 驗證，逐一標「本次未收集」；手動 full 與發布保持全量 192 個。兩個範圍歷史分開，cloud 過關不能表示完整或發布成功。範圍證據於 2026-11-13 到期，需重新查核。
- 每個範圍仍保留最多 8 個且 10% 的限制、持續解析／存取／瀏覽器錯誤阻擋及連續三次故障門檻；cloud 分母為 169，full 為 192。重要來源不得放入 Mac 專用範圍。發布檢查仍只在發布時啟動本機 Mac。
- Node Actions 更新至固定的 Node 24 相容版本，應用測試仍 Node 22；artifact 保留 7 天符合儲存庫限制。記錄 OS／Chrome／OpenSSL／curl 版本，不輸出環境密鑰。

## 相容性

192 個核准來源的完整集合不變，GUI／CLI 均保留來源，RSS 優先與共用篩選不變。JSON schema v4、Excel 17 欄、預設全文、相關性 hash 與國家標準時間契約保留。沒有把未收集來源、空列表或失敗當作成功。

## 驗證

前端檢查／建置、Rust 格式／完整測試／Clippy、CLI 精確來源集合及 Linux CI 均已通過初輪；新的範圍測試包含完整集合分割、重複／未知來源、缺證據、到期、重要來源不可排除，以及 cloud 綠色仍不能稱為全量／發布驗證。

修正後實際雲端收集結果將於完成後補入；上述成功 CI 與獨立網路診斷本身不表示實際收集已通過。
