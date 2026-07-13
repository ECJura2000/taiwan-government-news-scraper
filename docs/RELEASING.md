# 發行流程

本專案使用語意化版本（Semantic Versioning）。`pyproject.toml` 的 `[project].version` 是正式版本來源，例如 `1.1.0`；GitHub Release 與 Git tag 則使用 `vMAJOR.MINOR.PATCH` 格式，例如 `v1.1.0`。

合併版本變更到 `main` 後，GitHub Actions 會先在 Linux、Windows 與 macOS 建置單檔執行檔，執行全部 scraper registry 的封裝後 smoke test，並產生一份 SHA-256 清單與容量 manifest。三平台全部成功後，workflow 會依 `pyproject.toml` 自動建立對應 tag 與 GitHub Release。若該版本已存在，發布步驟會安全跳過，不會覆寫既有 Release。

仍可手動推送既有 `vMAJOR.MINOR.PATCH` tag；tag workflow 會驗證 tag 已存在後再發布。

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

確認 `CHANGELOG.md`、README 與必要的使用說明已更新，再調整 `pyproject.toml`：

```toml
[project]
version = "1.1.0"
```

將版本變更透過 PR 合併至 `main`。合併後的 `Build and release executables` workflow 即為正式發布程序，不需要另外手動建立 tag。

## 手動 tag 備援流程

只有在自動發布機制無法使用、且該版本尚未存在時，才採用：

```bash
git tag -a v1.1.0 -m "Release v1.1.0"
git push origin v1.1.0
```

## 自動化產物

成功後 Release 應包含六個檔案：

- `news-scraper-linux`
- `news-scraper-windows.exe`
- `news-scraper-macos`
- `news-scraper-v<版本>-SHA256SUMS.txt`
- `news-scraper-v<版本>-SIZE-MANIFEST.txt`

所有執行檔都必須先通過完整 runtime registry smoke test，Release job 才會發布。PyInstaller 會從 `news_scraper.scrapers.ministry` 遞迴收集機關爬蟲，新增來源不需維護靜態 import 清單。失敗時，workflow 另行保存 stdout、stderr 與 PyInstaller warning，供問題定位。

## 已知限制

執行檔未進行 Windows Authenticode 或 Apple Developer ID 簽章，作業系統可能顯示未知發行者警告。國土管理署來源使用 Selenium，執行環境仍須具備 Chrome 或 Chromium；PyInstaller 單檔不會內嵌瀏覽器。

若 release workflow 失敗，不應重複使用同一版本號覆蓋既有正式 Release。先修正問題，再遞增 PATCH 版本重新發行；已存在的正式 Release 原則上不應刪除或替換資產。
