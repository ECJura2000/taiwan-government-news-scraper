from datetime import datetime
from urllib.parse import urljoin

from ....config import INDSR_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import make_soup


def scrape_indsr_this_week():
    source = "國防院"
    soup = make_soup(fetch_html(URLS[source], timeout=INDSR_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("div.col-lg-6.card")
    if not rows:
        raise ValueError("國防院頁面找不到最新公告卡片 div.col-lg-6.card。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        a_tag = row.select_one("a[href]")
        title_tag = row.select_one("div.card-title")
        date_tag = row.select_one("p.time1")
        if not a_tag or not title_tag or not date_tag:
            continue
        try:
            news_date = datetime.strptime(clean_text(date_tag.get_text(" ", strip=True)).replace("/", ".").replace("-", "."), "%Y.%m.%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        title_text = clean_text(title_tag.get_text(" ", strip=True))
        if title_text and link:
            results.append(make_news_item(source, source, news_date, title_text, link))
    return results
