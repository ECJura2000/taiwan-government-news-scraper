# 部署與維護手冊

## 新增或修復來源

1. 在對應部會目錄新增或修復 parser。
2. 更新 `SCRAPER_SPECS`、來源 URL／順序／難度；`source_catalog.py` 會驗證一致性。
3. 加入 fixture、schema 與故障注入測試。
4. 執行 pytest、mypy、ruff 與小型 benchmark。

## AI 新十大建設規則

1. 修改 `config.py` 的 dataclass 規則後，確認主政來源存在、名稱唯一且關鍵字不重複。
2. 將新發現的真實正例、模糊例或誤判例加入 `tests/fixtures/ai_policy_labeled_titles.tsv`，保留官方來源網址。
3. 不得用時間留存集調整規則；新資料先加入回歸語料，定期另建新的時間留存集。
4. 執行 `python3 scripts/evaluate_ai_policy.py` 與時間留存集評估，確認整體 precision/recall 不低於 90%，各建設有樣本時不低於 85%。
5. 需要驗證來源內容時加上 `--verify-sources`，人工檢查 `title_not_found` 與 `fetch_failed`，不要讓網路結果阻擋一般 CI。
6. 規則行為變更時調升 `AI_POLICY_RULESET_VERSION`；執行報告會自動記錄版本與規則雜湊。

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
