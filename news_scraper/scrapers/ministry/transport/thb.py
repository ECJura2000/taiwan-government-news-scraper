import re
from urllib.parse import urljoin

from ....config import THB_LIST_TIMEOUT, URLS
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import By, EC, WebDriverWait, create_selenium_driver, load_selenium_page, make_soup


def scrape_thb_this_week():
    source = "公路局"
    driver = create_selenium_driver()
    try:
        html = load_selenium_page(
            driver,
            URLS[source],
            wait_condition=lambda d: WebDriverWait(d, THB_LIST_TIMEOUT).until(
                EC.presence_of_element_located((By.CSS_SELECTOR, "#table_0 tbody tr"))
            ),
            sleep_seconds=2,
        )
    finally:
        driver.quit()

    soup = make_soup(html)
    rows = soup.select("#table_0 tbody tr")
    if not rows:
        raise ValueError("公路局頁面找不到 #table_0 tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        title_td = tr.select_one('td[data-title="標題"]')
        department_td = tr.select_one('td[data-title="公告單位"]')
        date_td = tr.select_one('td[data-title="公告日期"]')
        if not title_td or not date_td:
            continue
        a_tag = title_td.select_one("a[href]")
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
        title_text = re.sub(r"^點擊前往\s*", "", title_text)
        department_text = clean_text(department_td.get_text(" ", strip=True)) if department_td else ""
        date_text = clean_text(date_td.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if not title_text or not date_text or not link:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week or news_date > end_of_week:
            continue
        department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
