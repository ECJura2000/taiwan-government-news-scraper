各機關新聞整理
================

1. 請將整個「各機關新聞」資料夾解壓縮到桌面或其他可寫入的位置。
2. 請先用 Release 的 SHA256SUMS.txt 核對下載 ZIP：

   Windows：Get-FileHash <ZIP檔名> -Algorithm SHA256
   macOS：shasum -a 256 <ZIP檔名>
   Linux：sha256sum <ZIP檔名>

3. 一般使用者可直接開啟「各機關新聞整理」執行檔。
4. Codex、排程或命令列可使用：

   Windows：各機關新聞整理.exe --headless --json-summary
   Linux/macOS：./各機關新聞整理 --headless --json-summary

5. Excel 會放在「新聞搜集區」，JSON 報告會放在其下的「執行紀錄」。
6. GUI 的「主題與關鍵字」可自由新增、刪除、匯入、匯出及測試分析規則。
7. 主題設定保存於「程式資料/relevance-profile.json」，GUI 與 headless 會共用。
8. 國土管理署來源需要系統已安裝 Chrome 或 Chromium。
9. 執行檔尚未使用 Windows Authenticode 或 Apple Developer ID 簽章。核對雜湊後，請依「啟動圖解」資料夾內的作業系統圖解開啟。
10. 若要讓 AI 閱讀或修改程式碼，請下載固定版本原始碼並依下列指南建立 Python 環境：
    https://github.com/ECJura2000/taiwan-government-news-scraper/blob/main/docs/AI_AUTOMATION.md

不要將 webhook、密碼或其他憑證寫入 settings.json 或 relevance-profile.json。
