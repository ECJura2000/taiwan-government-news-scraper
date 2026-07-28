# 部署與維護手冊

## 新增或修復來源

1. 在對應部會目錄新增或修復 parser。
2. 更新 `SCRAPER_SPECS`、來源 URL／順序／難度；`source_catalog.py` 會驗證一致性。
3. 加入 fixture、schema 與故障注入測試。
4. 執行 pytest、mypy、ruff 與小型 benchmark。

## 主題關聯性規則

1. 一般使用者透過 GUI 的「主題與關鍵字」管理規則，不必修改程式碼。
2. 共用設定位於 `程式資料/relevance-profile.json`；匯入設定前會顯示差異，確認後備份現有版本。
3. 內建 AI 十大建設範本的來源仍在 `config.py`，只供首次啟動、恢復範本及既有回歸測試使用。
4. 新增內建範本項目時必須使用穩定 ID 並測試升級合併；已刪除預設 ID 不得因升級重新出現。
5. 將新發現的 AI 範本正例、模糊例或誤判例加入 `tests/fixtures/ai_policy_labeled_titles.tsv`，保留官方來源網址。
6. 不得用時間留存集調整範本；新資料先加入回歸語料，定期另建新的時間留存集。
7. 執行 `python3 scripts/evaluate_ai_policy.py` 與時間留存集評估，確認既有範本的 precision/recall 不低於既定門檻。
8. 需要驗證來源內容時加上 `--verify-sources`，人工檢查 `title_not_found` 與 `fetch_failed`，不要讓網路結果阻擋一般 CI。
9. 範本內容變更時調升 `AI_POLICY_RULESET_VERSION`；執行報告以 `relevance_policy` 記錄設定名稱、版本與有效規則雜湊。

## 依賴與效能

- 更新 `requirements.lock.txt` 與 `requirements-dev.lock.txt`。
- 執行 `python3 scripts/benchmark_capacity.py --sizes 1000 10000 100000`。
- 檢查 CI benchmark artifact 與 observability budget warning。
- CI 會和 `benchmarks/baseline.json` 比較；只有確認效能變更合理後才更新基準。
- 每日非阻擋 `Source smoke` workflow 會檢查代表性外部來源。
- 至少連續兩週使用 `python3 scripts/record_long_term_run.py --input <result.json>` 累積真實執行證據後，再提出穩定性結論。

## 排程維護

解析／驗證錯誤不應自動重試；下載與 timeout 才進入第二輪。來源長期零筆或 P95
超出預算時，先檢查來源格式與健康報告，再調整 worker 或 timeout。
