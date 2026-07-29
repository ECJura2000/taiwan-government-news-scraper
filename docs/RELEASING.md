# 發行流程

`pyproject.toml` 的 `[project].version` 是唯一版本來源。正式 tag 使用 `vMAJOR.MINOR.PATCH`；GUI 等向下相容功能遞增 `MINOR`，錯誤修正遞增 `PATCH`。

## 發行前檢查

```bash
python3 -m pip install --require-hashes -r requirements.lock.txt
python3 -m pip install --require-hashes -r requirements-dev.lock.txt
python3 -m pip install --require-hashes -r requirements-security.lock.txt
python3 -m pip install --no-deps -e .
python3 -m ruff check .
python3 -m mypy
python3 -m compileall -q news_scraper build_entry.py scripts
python3 -m pytest -q
python3 scripts/benchmark_excel_export.py --rows 10000 --max-seconds 30
python3 -m bandit -q -ll -r news_scraper scripts -x tests
python3 -m pip_audit -r requirements.lock.txt --require-hashes
python3 scripts/check_workflows.py
```

版本、`CHANGELOG.md`、README 與可攜版說明必須在 PR 內一起更新。PR 會建置 Windows、Linux、macOS Apple Silicon 與 macOS Intel 四種短期 artifact，但不建立 Release。

## 一鍵正式發布

1. 將 PR 合併至 `main`。
2. 在 GitHub Actions 開啟 `Build and release portable apps`。
3. 選擇 `main` 並按 `Run workflow`。

Workflow 會重新驗證原始碼，平行建置 Windows、Linux、macOS Apple Silicon 與 macOS Intel 單檔，執行 registry、runtime、headless、GUI 與離線 browser smoke test，再建立四個可攜 ZIP、固定 AI 原始碼 ZIP、SHA-256 清單及容量 manifest。版本 tag 或 Release 已存在時會失敗，絕不覆蓋既有資產。普通 `main` push 不發布，避免未經確認的版本與重複 Actions 用量。

## Tag 備援

只有手動 workflow 無法使用且版本從未發布時才推送 tag：

```bash
git tag -a v1.5.2 -m "Release v1.5.2"
git push origin v1.5.2
```

Tag 必須與 `pyproject.toml` 完全一致。

## Release 產物

- `taiwan-government-news-v<版本>-linux.zip`
- `taiwan-government-news-v<版本>-windows.zip`
- `taiwan-government-news-v<版本>-macos-arm64.zip`
- `taiwan-government-news-v<版本>-macos-x64.zip`
- `taiwan-government-news-v<版本>-source.zip`
- `taiwan-government-news-v<版本>-SHA256SUMS.txt`
- `taiwan-government-news-v<版本>-SIZE-MANIFEST.txt`

每個可攜 ZIP 內含 `各機關新聞/各機關新聞整理[.exe]`、使用說明、首次啟動圖解、內部 SHA-256、`程式資料/` 與 `新聞搜集區/`。固定 AI 原始碼 ZIP 包含 `AGENTS.md`、自動化指南、完整 tracked source 與四份含雜湊鎖檔。每平台容量與最大 30 項模組報告只保留在一天期的 Actions artifact。

每個平台 ZIP 必須較 v1.5.0 基準縮小至少 15%，單一執行檔不得超過 65 MiB，全部 Release 資產（含 AI 原始碼 ZIP）不得超過 170 MiB。超過 100 KiB 的既有封裝項目若無說明成長超過 5%，建置會失敗。

## 已知限制

執行檔未進行 Windows Authenticode 或 Apple Developer ID 簽章，可能出現未知發行者警告。國土管理署仍需要系統 Chrome 或 Chromium。Windows、Linux、macOS Apple Silicon 與 macOS Intel 必須分別在原生 runner 建置，不能用單一作業系統交叉產生。
