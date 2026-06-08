from datetime import datetime
from urllib.parse import urljoin

from ....config import TOURISM_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import make_soup


def scrape_tourism_this_week():
    source = "觀光署"
    soup = make_soup(fetch_html(URLS[source], timeout=TOURISM_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("div.columnBlock")
    if not rows:
        raise ValueError("觀光署頁面找不到 div.columnBlock。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for block in rows:
        a_tag = block.select_one("a.columnBlock-title[href]")
        date_tag = block.select_one("span.date")
        if not a_tag or not date_tag:
            continue
        date_text = clean_text(date_tag.get_text(" ", strip=True))
        title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
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
        results.append(make_news_item(source, source, news_date, title_text, link))
    return results
