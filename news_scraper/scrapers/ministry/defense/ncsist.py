import re
from urllib.parse import urljoin

from ....config import NCSIST_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_ncsist_this_week():
    source = "中科院"
    soup = make_soup(fetch_html(URLS[source], timeout=NCSIST_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("div.newsNavArea ul")
    if not rows:
        raise ValueError("中科院頁面找不到新聞列表 div.newsNavArea ul。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        date_tag = row.select_one("li.newsTit02")
        a_tag = row.select_one("li.newsTit03 a[href]")
        if not date_tag or not a_tag:
            continue
        match = re.search(r"(\d{2,3})年(\d{1,2})月(\d{1,2})日", clean_text(date_tag.get_text(" ", strip=True)))
        if not match:
            continue
        try:
            news_date = roc_to_ad_date("{}-{}-{}".format(match.group(1), match.group(2), match.group(3)))
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if title_text and link:
            results.append(make_news_item(source, source, news_date, title_text, link))
    return results
