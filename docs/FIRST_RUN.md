# First run

Download the platform ZIP from GitHub Releases and verify it against `SHA256SUMS.txt`. The applications are not currently Authenticode or Apple Developer ID signed, so only bypass an operating-system warning after confirming the repository, filename and checksum.

- Windows: run the NSIS installer under `installers/`, or use `news-scraper.exe collect`.
- macOS: choose Apple Silicon (`macos-arm64`) or Intel (`macos-x64`), open the DMG under `installers/`, or use `./news-scraper collect`.
- Linux: use the AppImage or DEB under `installers/`, or use `./news-scraper collect`.

Install Chrome or Chromium for rendered official sources. The output location must be writable.
