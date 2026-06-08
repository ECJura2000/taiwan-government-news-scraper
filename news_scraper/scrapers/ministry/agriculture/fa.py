from urllib.parse import urljoin

from ....config import FA_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_fa_this_week():
    source = "漁業署"
    soup = make_soup(fetch_html(URLS[source], timeout=FA_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("tbody tr")
    if not rows:
        raise ValueError("漁業署頁面找不到 tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 2:
            continue
        a_tag = tds[0].select_one("a[href]")
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
        date_text = clean_text(tds[-1].get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if not title_text or not date_text or not link:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        results.append(make_news_item(source, source, news_date, title_text, link))
    return results
