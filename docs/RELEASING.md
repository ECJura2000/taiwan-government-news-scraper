# 發行流程

本專案使用語意化版本（Semantic Versioning）與 `vMAJOR.MINOR.PATCH` Git tag，例如 `v1.1.0`。推送符合格式的 tag 後，GitHub Actions 會在 Linux、Windows 與 macOS 建置單檔執行檔，執行封裝後 smoke test，產生 SHA-256，並建立 GitHub Release。

## 版本號判斷

- `MAJOR`：存在不相容的 CLI、輸出格式或設定變更。
- `MINOR`：新增向下相容的來源、功能或輸出能力。
- `PATCH`：修正錯誤、解析規則、文件或內部工程品質，且不破壞既有介面。

## 提交訊息

建議採用 Conventional Commits：

```text
feat: add a new government news source
fix: repair date parsing for an existing source
docs: clarify executable usage
ci: harden release artifact validation
refactor: simplify scraper registry loading
```

破壞性變更應在類型後加入 `!`，並在提交本文加入 `BREAKING CHANGE:` 說明。

## 發行前檢查

在乾淨環境執行：

```bash
python3 -m pip install -e ".[dev]"
python3 -m ruff check .
python3 -m mypy
python3 -m compileall -q news_scraper build_entry.py
python3 -m pytest -q --cov=news_scraper.quality --cov=news_scraper.utils.text --cov-fail-under=80
python3 -m news_scraper --list-sources
```

確認 `CHANGELOG.md`、README 與必要的使用說明已更新，再建立並推送 tag：

```bash
git tag -a v1.1.0 -m "Release v1.1.0"
git push origin v1.1.0
```

## 自動化產物

成功後 Release 應包含六個檔案：

- `news-scraper-linux`
- `news-scraper-linux.sha256`
- `news-scraper-windows.exe`
- `news-scraper-windows.exe.sha256`
- `news-scraper-macos`
- `news-scraper-macos.sha256`

所有執行檔都必須先通過 `--list-sources` smoke test，Release job 才會發布。

## 已知限制

執行檔未進行 Windows Authenticode 或 Apple Developer ID 簽章，作業系統可能顯示未知發行者警告。國土管理署來源使用 Selenium，執行環境仍須具備 Chrome 或 Chromium；PyInstaller 單檔不會內嵌瀏覽器。

若 release workflow 失敗，不應重複使用同一版本號覆蓋既有正式 Release。先修正問題，再刪除尚未發布或錯誤的 tag，或遞增 PATCH 版本重新發行。
