import re
from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ...base import make_soup


def scrape_mofa_this_week():
    source = "外交部"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("table tbody tr")
    if not rows:
        raise ValueError("外交部頁面找不到任何 table tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        row_text = tr.get_text(" ", strip=True)
        date_match = re.search(r"\b(\d{4}-\d{2}-\d{2})\b", row_text)
        a_tag = tr.select_one("a[href]")
        if not date_match or not a_tag:
            continue
        title_text = a_tag.get_text(" ", strip=True)
        if not title_text:
            continue
        try:
            news_date = datetime.strptime(date_match.group(1), "%Y-%m-%d").date()
        except ValueError:
            continue
        if start_of_week <= news_date <= end_of_week:
            results.append(
                make_news_item(
                    source,
                    source,
                    news_date,
                    title_text,
                    urljoin(URLS[source], a_tag.get("href", "").strip()),
                )
            )
    return results
