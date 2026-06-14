# 台灣政府新聞爬蟲專題報告

## 問題與需求

政府新聞來源分散且格式不一致。本系統定期抓取、解析、去重、品質檢查並輸出 Excel，同時保留來源健康、失敗原因與效能資料。

## 架構與資料結構

來源 catalog 決定排程，thread pool 執行抓取，parser 轉換為 `NewsItem`，`dict`／`set` 完成去重與來源統計，最後輸出 Excel 與執行摘要。詳見 `ARCHITECTURE.md`。

## 複雜度、效能與驗證

去重與品質掃描平均 O(n)，排序 O(n log n)。容量結果見 `PERFORMANCE.md`。驗證包含 fixture、整合、property、故障注入、coverage、mypy、Ruff、安全掃描、benchmark 與每日來源 smoke test。

## 限制與未來工作

外部網站可能改版、阻擋自動化或暫時停機。未來應持續縮小來源 parser 的廣泛例外、累積長期成功率並完成真人 UAT。
