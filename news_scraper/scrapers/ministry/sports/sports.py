from urllib.parse import urljoin

from ....config import SPORTS_RSS_TIMEOUT, URLS
from ....models import make_news_item
from ....utils.dates import roc_to_ad_date
from ....utils.text import clean_text
from ...base import By, EC, WebDriverWait, collect_weekly_results_from_ordered_rows, create_selenium_driver, load_selenium_page, make_soup


def scrape_sports_this_week():
    source = "運動部"
    driver = create_selenium_driver()
    try:
        load_selenium_page(
            driver,
            URLS[source],
            wait_condition=lambda d: WebDriverWait(d, SPORTS_RSS_TIMEOUT + 10).until(
                EC.presence_of_element_located((By.CSS_SELECTOR, "table tbody tr"))
            ),
        )
        driver.execute_script(
            """
            const select = document.querySelector('#InputPageSize');
            if (select) {
                select.value = '500';
                select.dispatchEvent(new Event('change', { bubbles: true }));
            }
            """
        )
        WebDriverWait(driver, SPORTS_RSS_TIMEOUT + 10).until(
            lambda d: len(d.find_elements(By.CSS_SELECTOR, "table tbody tr")) > 20
        )
        soup = make_soup(driver.page_source)
        rows = [row for row in soup.select("table tbody tr") if row.select("td")]
        if not rows:
            raise ValueError("運動部列表頁找不到新聞列。")

        def extract_row_date(row):
            date_td = row.select_one('td[data-title="上版日期"] div.in')
            if not date_td:
                return None
            try:
                return roc_to_ad_date(clean_text(date_td.get_text(" ", strip=True)))
            except Exception:
                return None

        def build_row_item(row, news_date):
            title_td = row.select_one('td[data-title="主題"] a[href]')
            if not title_td:
                return None
            department_text = clean_text(row.select_one('td[data-title="資料來源"] div.in').get_text(" ", strip=True)) if row.select_one('td[data-title="資料來源"] div.in') else ""
            title_text = clean_text(title_td.get("title", "") or title_td.get_text(" ", strip=True))
            link = urljoin(URLS[source], clean_text(title_td.get("href", "")))
            if not title_text or not link:
                return None
            department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
            return make_news_item(source, department_label, news_date, title_text, link)

        results, _ = collect_weekly_results_from_ordered_rows(rows, extract_row_date, build_row_item)
        return results
    finally:
        driver.quit()
