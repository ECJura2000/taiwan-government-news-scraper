# 災難復原演練

1. 使用測試輸出目錄，不操作唯一正式輸出。
2. 模擬錯誤 JSON、網路 timeout、Excel 寫入失敗及程序中止。
3. 執行 `python3 -m pytest tests/test_fault_injection.py tests/test_integration_pipeline.py tests/test_monitoring.py`。
4. 確認舊 Excel／摘要未被破壞，下載錯誤才會重試。
5. 記錄演練日期、復原時間、資料損失與改善事項。
