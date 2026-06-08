import re
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup


def scrape_hakka_this_week():
    source = "客委會"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("div.list ul li")
    if not rows:
        raise ValueError("客委會頁面找不到 div.list ul li。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in rows:
        date_tag = li.select_one("p.color02")
        title_tag = li.select_one("p.subject")
        a_tag = li.select_one("a[href]")
        if not date_tag or not title_tag or not a_tag:
            continue
        match = re.search(r"(\d{3}-\d{2}-\d{2})", date_tag.get_text(strip=True))
        if not match:
            continue
        try:
            news_date = roc_to_ad_date(match.group(1))
        except Exception:
            continue
        if not (start_of_week <= news_date <= end_of_week):
            continue
        results.append(
            make_news_item(
                source,
                source,
                news_date,
                title_tag.get_text(" ", strip=True),
                urljoin(URLS[source], a_tag.get("href", "").strip()),
            )
        )
    return results
