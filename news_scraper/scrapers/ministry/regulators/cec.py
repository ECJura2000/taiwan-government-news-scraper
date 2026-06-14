import re

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup


def scrape_cec_this_week():
    source = "中選會"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("div.article-item")
    if not rows:
        raise ValueError("中選會頁面找不到 div.article-item。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        text = row.get_text(" ", strip=True)
        match = re.search(r"\d{3}\.\d{2}\.\d{2}", text)
        if not match:
            continue
        try:
            news_date = roc_to_ad_date(match.group(0).replace(".", "-"))
        except ValueError:
            continue
        if start_of_week <= news_date <= end_of_week:
            results.append(make_news_item(source, source, news_date, text.replace(match.group(0), "").strip(), URLS[source]))
    return results
