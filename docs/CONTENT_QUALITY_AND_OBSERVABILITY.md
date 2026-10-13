# 全文品質與執行診斷

## 正文解析

2026-10-11 以官方頁面核對行政院、高速公路局、交通部、外交部、教育部、文化部、勞動部、金管會、海巡署的正文容器，固定樣本放在 `src-tauri/tests/fixtures/*_detail.html`。偵防分署共用海巡署的正文結構。只擷取新聞容器，排除腳本、導覽、頁尾、圖片說明區及操作控制；不以整頁文字冒充全文。金管會 RSS CDATA 的 XML 查詢分隔符在網址解析前解碼，避免新聞連結落入列表頁。已確認只有 PDF 附件、沒有 HTML 正文的金管會頁面記為 `attachment_only`，保留新聞且不將附件名稱冒充全文；目前不另擷取 PDF 內容。

## 來源別品質門檻

來源目錄新增選用 `detail_quality`：

```json
{"expectation":"full_text","min_items":5,"min_coverage_percent":70}
```

已核對正文結構的上述十個來源，在完整模式的**最後保留新聞**達到五篇而全文覆蓋率低於 70% 時，產生 `detail_coverage_below_minimum` 警示，並在 `anomalies` 指出來源。報告的 `quality.detail_quality` 保留門檻、分母、全文數和判定結果。恰好 70% 不警示，零篇或不足五篇記為 `insufficient_sample`；來源失敗另依來源健康判定。摘要模式不啟用全文門檻。

`expectation` 也支援 `summary_only`，供已確認只有摘要的來源使用；目前未將其他來源任意歸類為摘要來源。未宣告門檻的來源沿用既有歷史退步診斷。門檻不刪除新聞，也不跳過全文補取；既有 `content_warnings` 和全文失敗診斷繼續保留。

## 耗時與請求證據

`performance.stage_wall_seconds` 是互不重疊的連續階段，包括初始化、蒐集、聚合、品質整理、排名、shadow、模式比較、索引與主題篩選、Excel、品質與新聞紀錄、摘要組裝。相加應接近 `monotonic_duration_seconds`。原有來源／全文累計時間仍可能重疊，不能相加當成總時間。

`duration_seconds` 維持開始與結束 UTC 時間之差；`wall_clock_gap_seconds` 是它與單調時鐘的差。系統睡眠或時鐘調整可能造成差額，不能僅憑差額斷言網路等待。開始／結束時間現在涵蓋新聞紀錄和摘要組裝；JSON 序列化、fsync 及雜湊另記在 `.complete` manifest 的 `performance`。發布動作與 CLI runtime 結束不包含在報告總時間內。

`performance.http` 記錄應用程式送出請求次數、重試、304 快取命中、條件式重驗、主機等待及重試等待。重導子請求與 Chrome 自行發出的請求不在此計數範圍；等待為請求累計值，可能重疊。每篇全文補取的 HTTP、瀏覽器備援與解析時間保留在 `source_diagnostics[].detail_fetch.requests`。

## 日常來源檢查

`source-smoke-gate.mjs` 共用發布門檻：對首次失敗來源做一次單來源重測後，尚未恢復的來源不得超過八個，失敗比例不得超過 10%。持續解析、拒絕存取及瀏覽器錯誤仍阻擋；全文品質警示也阻擋。來源成功而零篇不算失敗。

同一來源連續三次完整檢查（包含重測）仍失敗時，阻擋並列出次數。成功或重測恢復會歸零，同一 run ID 不重複累加。歷史由前一次同分支 `rust-source-smoke` artifact 讀回；產物保留 7 天，即使門檻失敗仍上傳歷史。工作流程序列化，避免平行執行覆寫歷史。首次啟用或找不到保留歷史時，工作流程會顯示重新建立歷史的 notice；無法讀回或歷史格式錯誤會失敗，不靜默忽略。

核准來源集合仍完整比對 `approved_sources.json`（192 個）。日常預設 `cloud` 範圍依 `source_verification_profiles.json` 的逐來源證據執行 169 個；23 個在兩次 GitHub 查核連線逾時／驗證頁阻擋而 Mac 全量成功的來源，明示「本次未收集、須由 Mac 驗證」，不算本次成功，也不從 GUI、CLI 或發布清單移除。範圍證據於 2026-11-13 必須複查，到期或證據缺漏會阻擋 cloud 工作，不靜默沿用；不得把重要來源移入此清單。手動 `full` 仍嘗試全部 192 個，原始失敗分類及門檻保留。

`environment-summary.json`／`.md` 與 Actions 摘要區分選取範圍、完整来源是否驗證及門檻結果。cloud 的綠色門檻只代表其明確選取範圍符合原有最多八個且 10% 的限制；分母是實際範圍 169，完整檢查仍為 192，沒有提高允許失敗比例。解析、拒絕存取、Chrome 執行錯誤及連續三次未恢復仍阻擋。cloud/full 的歷史分別存於 `rust-source-smoke-cloud`／`rust-source-smoke`，不混合計算。

日常雲端檢查不能代替發布檢查。發布仍需發布時才啟動、只接一個工作的本機 Mac 執行器，以全部 192 個核准來源及原嚴格門檻驗證。雲端產物沒有配對的本機發布證據時，`release_verification.status` 固定為 `not_evaluated`；cloud 永遠不能宣稱全目錄或發布驗證成功，也不新增永久 Mac 排程。

本次沒有修改核准來源集合（192 個）、CLI 選項、JSON schema v4、17 欄 Excel、預設 full／prefilter off 或國防部 TLS 例外。OS、Chrome／Chromium、OpenSSL、curl 版本另存 `environment-versions.json`，不輸出憑證或環境變數。新增診斷不代表發布檢查已通過。
