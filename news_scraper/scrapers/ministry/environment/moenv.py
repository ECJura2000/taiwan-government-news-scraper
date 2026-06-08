from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup


def scrape_moenv_this_week():
    source = "環境部"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("ul.list_group li")
    if not rows:
        raise ValueError("環境部頁面找不到 ul.list_group li。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in rows:
        a_tag = li.select_one("a[href]")
        title_tag = li.select_one("div.title")
        date_tag = li.select_one("span.date")
        if not a_tag or not title_tag or not date_tag:
            continue
        try:
            news_date = roc_to_ad_date(date_tag.get_text(strip=True))
        except Exception:
            continue
        if start_of_week <= news_date <= end_of_week:
            results.append(
                make_news_item(
                    source,
                    source,
                    news_date,
                    title_tag.get_text(" ", strip=True),
                    urljoin("https://www.moenv.gov.tw/", a_tag.get("href", "").strip()),
                )
            )
    return results
