from urllib.parse import urljoin

from ....config import JUDICIAL_LIST_TIMEOUT, JUDICIAL_MAX_PAGES, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import build_department_label, clean_text
from ...base import make_soup


def build_judicial_yuan_page_url(page_number):
    return URLS["司法院"].replace("lp-1790-1-1-40.html", "lp-1790-1-{}-40.html".format(page_number))


def extract_judicial_yuan_row(row, source):
    cells_by_title = {}
    for cell in row.find_all("td", recursive=False):
        title = clean_text(cell.get("data-title", ""))
        if title:
            cells_by_title[title] = cell

    title_cell = cells_by_title.get("標題")
    date_cell = cells_by_title.get("張貼日")
    unit_cell = cells_by_title.get("單位/機關")
    if title_cell is None or date_cell is None:
        return None

    a_tag = title_cell.find("a", href=True)
    title_text = clean_text(a_tag.get_text(" ", strip=True)) if a_tag else clean_text(title_cell.get_text(" ", strip=True))
    date_text = clean_text(date_cell.get_text(" ", strip=True))
    unit_text = clean_text(unit_cell.get_text(" ", strip=True)) if unit_cell else source
    if not title_text or not date_text:
        return None

    try:
        news_date = roc_to_ad_date(date_text)
    except Exception:
        return None

    link = urljoin(URLS[source], a_tag.get("href", "").strip()) if a_tag else URLS[source]
    department_label = build_department_label(source, unit_text, aliases={source})
    return news_date, make_news_item(source, department_label, news_date, title_text, link)


def scrape_judicial_yuan_this_week():
    source = "司法院"
    start_of_week, end_of_week = get_cached_week_range()
    results = []

    for page_number in range(1, JUDICIAL_MAX_PAGES + 1):
        soup = make_soup(fetch_html(build_judicial_yuan_page_url(page_number), timeout=JUDICIAL_LIST_TIMEOUT))
        rows = soup.select("table.table_list tbody tr")
        if not rows:
            rows = soup.select("div.table_list table tbody tr")
        if not rows and page_number == 1:
            raise ValueError("司法院頁面找不到新聞列表 table.table_list tbody tr。")

        reached_old_news = False
        for row in rows:
            extracted = extract_judicial_yuan_row(row, source)
            if extracted is None:
                continue

            news_date, item = extracted
            if news_date < start_of_week:
                reached_old_news = True
                break
            if news_date > end_of_week:
                continue
            results.append(item)

        if reached_old_news:
            break

    return results
