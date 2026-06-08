from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ...base import make_soup


def scrape_cbc_this_week():
    source = "中央銀行"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("section.lp div.list ul li")
    if not rows:
        raise ValueError("央行頁面找不到 section.lp div.list ul li。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in rows:
        date_tag = li.select_one("time")
        a_tag = li.select_one("a[href]")
        if not date_tag or not a_tag:
            continue
        try:
            news_date = datetime.strptime(date_tag.get_text(strip=True), "%Y-%m-%d").date()
        except ValueError:
            continue
        if start_of_week <= news_date <= end_of_week:
            results.append(
                make_news_item(
                    source,
                    source,
                    news_date,
                    a_tag.get_text(" ", strip=True),
                    urljoin(URLS[source], a_tag.get("href", "").strip()),
                )
            )
    return results
