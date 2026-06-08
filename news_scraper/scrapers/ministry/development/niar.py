from datetime import datetime
from urllib.parse import urljoin

from ....config import NIAR_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import make_soup


def scrape_niar_this_week():
    source = "國家實驗研究院"
    soup = make_soup(
        fetch_html(URLS[source], timeout=NIAR_LIST_TIMEOUT, extra_headers={"Connection": "close"})
    )
    rows = soup.select("table.rwdTable tr")
    if not rows:
        raise ValueError("國家實驗研究院頁面找不到 table.rwdTable tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        date_td = row.select_one("td.date")
        title_td = row.select_one("td.title")
        a_tag = title_td.select_one("a[href]") if title_td else None
        if not date_td or not a_tag:
            continue

        date_text = clean_text(date_td.get_text(" ", strip=True))
        try:
            news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue

        title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if title_text and link:
            results.append(make_news_item(source, source, news_date, title_text, link))
    return results
