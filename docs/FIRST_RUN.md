# First run

Download the platform ZIP from GitHub Releases and verify it against `SHA256SUMS.txt`. The applications are not currently Authenticode or Apple Developer ID signed, so only bypass an operating-system warning after confirming the repository, filename and checksum.

- Windows: download and run `TaiwanGovernmentNews-Setup-v2.1.7.exe` for normal use. If you need the portable ZIP, extract it completely and double-click the top-level `各機關新聞整理.exe`; use `cli/news-scraper.exe collect` only for advanced CLI work.
- macOS: choose Apple Silicon (`macos-arm64`) or Intel (`macos-x64`), open the DMG under `installers/`, or use `./news-scraper collect`.
- Linux: use the AppImage or DEB under `installers/`, or use `./news-scraper collect`.

Install Chrome or Chromium for rendered official sources. The output location must be writable.
