# First run

Download the platform ZIP from GitHub Releases and verify it against `SHA256SUMS.txt`. The applications are not currently Authenticode or Apple Developer ID signed, so only bypass an operating-system warning after confirming the repository, filename and checksum.

- Windows: download and run `TaiwanGovernmentNews-Setup-v2.1.10.exe` for normal use. If you need the portable ZIP, extract it completely and double-click the top-level `各機關新聞整理.exe`; use `cli/news-scraper.exe collect` only for advanced CLI work.
- macOS: choose Apple Silicon (`macos-arm64`) or Intel (`macos-x64`), extract the ZIP, then open the top-level `各機關新聞整理.app`. If macOS says the app is damaged or cannot be opened, run the bundled `解除封鎖並開啟.command`, or right-click `各機關新聞整理.app` in Finder and choose Open. The CLI remains available as `./news-scraper collect`.
- Linux: use the AppImage or DEB under `installers/`, or use `./news-scraper collect`.

Install Chrome, Chromium or Microsoft Edge for rendered official sources. Windows standard system and per-user install locations are detected automatically; set `NEWS_SCRAPER_CHROME_BIN` to the browser executable only for a non-standard installation. The output location must be writable.
