from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup


def scrape_vac_this_week():
    source = "退輔會"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("section.listTb table tbody tr")
    if not rows:
        raise ValueError("退輔會頁面找不到 table tbody tr。")
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        title_td = tr.select_one('td[data-title="標題"]')
        date_td = tr.select_one('td[data-title="發布日期"]')
        if not title_td or not date_td:
            continue
        a_tag = title_td.select_one("a[href]")
        if not a_tag:
            continue
        try:
            news_date = roc_to_ad_date(date_td.get_text(strip=True))
        except Exception:
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
