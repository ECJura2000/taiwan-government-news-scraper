from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ...base import fetch_page_summary, make_soup


def scrape_nstc_this_week():
    source = "國科會"
    soup = make_soup(fetch_html(URLS[source]))
    container = soup.select_one("div.news_list.marb_30")
    if not container:
        raise ValueError("國科會頁面找不到 div.news_list.marb_30。")
    rows = container.select("a[href]")
    if not rows:
        raise ValueError("國科會頁面找不到任何 a[href]。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for a_tag in rows:
        date_tag = a_tag.select_one("div.date")
        title_tag = a_tag.select_one("h3")
        if not date_tag or not title_tag:
            continue
        try:
            news_date = datetime.strptime(date_tag.get_text(strip=True), "%Y-%m-%d").date()
        except ValueError:
            continue
        if start_of_week <= news_date <= end_of_week:
            link = urljoin(URLS[source], a_tag.get("href", "").strip())
            results.append(
                make_news_item(
                    source,
                    source,
                    news_date,
                    title_tag.get_text(" ", strip=True),
                    link,
                    summary=fetch_page_summary(link, (".articleContent", ".article-content", "article")),
                )
            )
    return results
