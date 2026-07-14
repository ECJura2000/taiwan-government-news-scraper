from datetime import datetime
from urllib.parse import urljoin

from ....config import NLMA_LIST_TIMEOUT, URLS
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import By, EC, WebDriverWait, create_selenium_driver, load_selenium_page, make_soup


def scrape_nlma_this_week():
    source = "國土管理署"
    driver = create_selenium_driver()
    try:
        html = load_selenium_page(
            driver,
            URLS[source],
            wait_condition=lambda d: WebDriverWait(d, NLMA_LIST_TIMEOUT).until(
                EC.presence_of_element_located((By.CSS_SELECTOR, 'a[role="row"][href*="/ch/titlelist/news/"]'))
            ),
            sleep_seconds=2,
        )
    finally:
        driver.quit()

    soup = make_soup(html)
    rows = soup.select('a[role="row"][href*="/ch/titlelist/news/"]')
    if not rows:
        raise ValueError("國土管理署頁面找不到新聞列表列。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        date_tag = row.select_one('div[data-th="發布日期"] span')
        title_tag = row.select_one('div[data-th="標題"] span')
        dept_tag = row.select_one('div[data-th="單位分類"] span')
        if not date_tag or not title_tag:
            continue
        date_text = clean_text(date_tag.get_text(" ", strip=True))
        title_text = clean_text(title_tag.get_text(" ", strip=True))
        department_text = clean_text(dept_tag.get_text(" ", strip=True)) if dept_tag else ""
        link = urljoin(URLS[source], row.get("href", "").strip())
        if not date_text or not title_text or not link:
            continue
        try:
            news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
