from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ...base import make_soup


def scrape_ocac_this_week():
    source = "僑委會"
    soup = make_soup(fetch_html(URLS[source]))
    container = soup.select_one("ul.text_list")
    if not container:
        raise ValueError("僑委會頁面找不到 ul.text_list。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in container.select("li"):
        a_tag = li.select_one("a[href]")
        if not a_tag:
            continue
        text = a_tag.get_text(" ", strip=True)
        if len(text) < 10:
            continue
        date_text = text[:10]
        try:
            news_date = datetime.strptime(date_text, "%Y/%m/%d").date()
        except Exception:
            continue
        if not (start_of_week <= news_date <= end_of_week):
            continue
        results.append(
            make_news_item(
                source,
                source,
                news_date,
                text[len(date_text):].strip(),
                urljoin(URLS[source], a_tag.get("href", "").strip()),
            )
        )
    return results
