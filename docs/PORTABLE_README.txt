各機關新聞整理 v2.1.7（Rust）

1. 先以 GitHub Release 的 SHA256SUMS.txt 驗證本 ZIP。
2. Windows：一般使用者請優先下載並執行 TaiwanGovernmentNews-Setup-v2.1.7.exe。
3. Windows portable：完整解壓後雙擊頂層的 各機關新聞整理.exe；CLI 位於 cli/news-scraper.exe。
4. Headless CLI：
   Windows：cli\news-scraper.exe collect
   Linux/macOS：./news-scraper collect
5. 列出來源：news-scraper list-sources
6. 動態來源需要系統 Chrome 或 Chromium。
7. Excel 預設輸出到 新聞搜集區/；GUI 可用按鈕選資料夾。若只選 Excel 資料夾，JSON 位於該資料夾下的 執行紀錄/。

本封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。
