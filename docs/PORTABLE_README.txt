各機關新聞整理 v2.1.9（Rust）

1. 先以 GitHub Release 的 SHA256SUMS.txt 驗證本 ZIP。
2. Windows：一般使用者請優先下載並執行 TaiwanGovernmentNews-Setup-v2.1.9.exe。
3. Windows portable：完整解壓後雙擊頂層的 各機關新聞整理.exe；CLI 位於 cli/news-scraper.exe。
4. macOS：解壓縮後可先雙擊 解除封鎖並開啟.command；若系統仍跳安全提示，請在 Finder 對 各機關新聞整理.app 按右鍵選「打開」。
5. Headless CLI：
   Windows：cli\news-scraper.exe collect
   Linux/macOS：./news-scraper collect
6. 列出來源：news-scraper list-sources
7. 動態來源需要系統 Chrome、Chromium 或 Microsoft Edge；Windows 標準安裝位置會自動偵測，非標準安裝可設定 NEWS_SCRAPER_CHROME_BIN。
8. Excel 預設輸出到 新聞搜集區/；GUI 可用按鈕選資料夾。若只選 Excel 資料夾，JSON 位於該資料夾下的 執行紀錄/。

本封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。
