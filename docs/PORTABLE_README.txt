各機關新聞整理 v2.1.1（Rust）

1. 先以 GitHub Release 的 SHA256SUMS.txt 驗證本 ZIP。
2. GUI 安裝檔位於 installers/。
3. Headless CLI：
   Windows：news-scraper.exe collect
   Linux/macOS：./news-scraper collect
4. 列出來源：news-scraper list-sources
5. 動態來源需要系統 Chrome 或 Chromium。
6. Excel 預設輸出到 新聞搜集區/，JSON 位於 新聞搜集區/執行紀錄/。

本封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。
