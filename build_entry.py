"""Stable top-level entry point used by PyInstaller builds."""

import sys

from news_scraper.main import main


def configure_utf8_stdio() -> None:
    """Prevent Windows frozen apps from failing on redirected Chinese output."""

    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if not callable(reconfigure):
            continue
        try:
            reconfigure(encoding="utf-8", errors="backslashreplace")
        except (OSError, ValueError):
            continue


def check_bundled_runtime() -> int:
    """Verify third-party and dynamically loaded scraper modules are importable."""

    from news_scraper.runtime import validate_runtime_environment
    from news_scraper.scrapers.registry import SCRAPER_REGISTRY

    validate_runtime_environment()
    for source_name in SCRAPER_REGISTRY:
        SCRAPER_REGISTRY[source_name]
    forbidden_loaded = sorted(
        name
        for name in ("numpy", "pandas")
        if name in sys.modules
    )
    if forbidden_loaded:
        raise RuntimeError(
            "封裝執行環境不應載入：{}".format("、".join(forbidden_loaded))
        )
    print("封裝執行環境檢查通過；全部爬蟲模組檢查通過。")
    return 0


def check_browser_runtime() -> int:
    """Launch a local headless Chrome session without depending on a live website."""

    from selenium.webdriver.chrome.options import Options
    from selenium.webdriver.chrome.webdriver import WebDriver as ChromeWebDriver
    from selenium.webdriver.common.by import By
    from selenium.webdriver.support import expected_conditions as EC
    from selenium.webdriver.support.ui import WebDriverWait

    options = Options()
    for argument in (
        "--headless=new",
        "--disable-gpu",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--window-size=1024,768",
    ):
        options.add_argument(argument)
    driver = ChromeWebDriver(options=options)
    try:
        driver.get("data:text/html;charset=utf-8,<main id='browser-smoke'>ok</main>")
        element = WebDriverWait(driver, 10).until(
            EC.presence_of_element_located((By.ID, "browser-smoke"))
        )
        if element.text != "ok":
            raise RuntimeError("Chrome 測試頁內容不正確。")
    finally:
        driver.quit()
    print("Chrome／Selenium 離線檢查通過。")
    return 0


if __name__ == "__main__":
    configure_utf8_stdio()
    if "--check-runtime" in sys.argv[1:]:
        raise SystemExit(check_bundled_runtime())
    if "--browser-smoke-test" in sys.argv[1:]:
        raise SystemExit(check_browser_runtime())
    if "--gui-smoke-test" in sys.argv[1:]:
        from news_scraper.gui import main as gui_main

        raise SystemExit(gui_main(smoke_test=True))
    if not sys.argv[1:] or "--gui" in sys.argv[1:]:
        from news_scraper.gui import main as gui_main

        raise SystemExit(gui_main())
    raise SystemExit(main())
