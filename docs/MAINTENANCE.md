# 部署與維護手冊

## 新增或修復來源

1. 在對應部會目錄新增或修復 parser。
2. 更新 `SCRAPER_SPECS`、來源 URL／順序／難度；`source_catalog.py` 會驗證一致性。
3. 加入 fixture、schema 與故障注入測試。
4. 執行 pytest、mypy、ruff 與小型 benchmark。

## 依賴與效能

- 更新 `requirements.lock.txt` 與 `requirements-dev.lock.txt`。
- 執行 `python3 scripts/benchmark_capacity.py --sizes 1000 10000 100000`。
- 檢查 CI benchmark artifact 與 observability budget warning。
- CI 會和 `benchmarks/baseline.json` 比較；只有確認效能變更合理後才更新基準。
- 每日非阻擋 `Source smoke` workflow 會檢查代表性外部來源。

## 排程維護

解析／驗證錯誤不應自動重試；下載與 timeout 才進入第二輪。來源長期零筆或 P95
超出預算時，先檢查來源格式與健康報告，再調整 worker 或 timeout。
