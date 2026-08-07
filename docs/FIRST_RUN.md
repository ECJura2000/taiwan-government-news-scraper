# First run

Download the platform ZIP from GitHub Releases and verify it against `SHA256SUMS.txt`. The applications are not currently Authenticode or Apple Developer ID signed, so only bypass an operating-system warning after confirming the repository, filename and checksum.

- Windows: extract the whole ZIP, then double-click `START-GUI.cmd` (or `TaiwanGovernmentNews-GUI.exe`) for the portable GUI. If WebView2 is unavailable or local security policy blocks portable execution, run the NSIS installer under `installers/`. Use `news-scraper.exe collect` for the CLI.
- macOS: choose Apple Silicon (`macos-arm64`) or Intel (`macos-x64`), open the DMG under `installers/`, or use `./news-scraper collect`.
- Linux: use the AppImage or DEB under `installers/`, or use `./news-scraper collect`.

Install Chrome or Chromium for rendered official sources. The output location must be writable.
