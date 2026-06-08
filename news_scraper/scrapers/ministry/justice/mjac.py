from urllib.parse import urljoin

from ....config import MJAC_LIST_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_mjac_this_week():
    source = "矯正署"
    html = fetch_html_resilient(URLS[source], timeout=MJAC_LIST_TIMEOUT, extra_headers={"Connection": "close"})
    soup = make_soup(html)
    rows = soup.select("table.table_list tbody tr")
    if not rows:
        rows = soup.select("table tbody tr")
    if not rows:
        raise ValueError("矯正署頁面找不到可解析的新聞列表。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        a_tag = row.select_one("td a[href]")
        date_cell = row.select_one("td.date")
        if not a_tag or not date_cell:
            continue
        date_text = clean_text(date_cell.get_text(" ", strip=True)).replace("/", "-").replace(".", "-")
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if title_text and link:
            results.append(make_news_item(source, source, news_date, title_text, link))
    return results
