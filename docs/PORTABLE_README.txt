各機關新聞整理 v2.1.4（Rust）

1. 先以 GitHub Release 的 SHA256SUMS.txt 驗證本 ZIP。
2. Windows：完整解壓後雙擊 START-GUI.cmd，或直接執行 TaiwanGovernmentNews-GUI.exe。
3. GUI 安裝檔位於 installers/；缺少 WebView2 或攜帶版無法啟動時，請使用安裝檔。
4. Headless CLI：
   Windows：news-scraper.exe collect
   Linux/macOS：./news-scraper collect
5. 列出來源：news-scraper list-sources
6. 動態來源需要系統 Chrome 或 Chromium。
7. Excel 預設輸出到 新聞搜集區/，JSON 位於 新聞搜集區/執行紀錄/。

本封裝不含 Python runtime、PyInstaller、openpyxl 或 Selenium。
