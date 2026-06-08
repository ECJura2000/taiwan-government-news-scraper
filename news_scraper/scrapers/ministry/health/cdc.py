import re
from datetime import datetime
from urllib.parse import urljoin

from ....config import CDC_RSS_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import make_soup


def scrape_cdc_this_week():
    source = "疾管署"
    soup = make_soup(fetch_html(URLS[source], timeout=CDC_RSS_TIMEOUT))
    anchors = soup.select('a[href*="/Bulletin/Detail/"]')
    if not anchors:
        raise ValueError("疾管署頁面找不到新聞連結。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for a_tag in anchors:
        href = clean_text(a_tag.get("href", ""))
        title_text = clean_text(a_tag.get("title", "") or a_tag.get_text(" ", strip=True))
        year_text = clean_text(a_tag.select_one(".icon-year").get_text(" ", strip=True)) if a_tag.select_one(".icon-year") else ""
        day_text = clean_text(a_tag.select_one(".icon-date").get_text(" ", strip=True)) if a_tag.select_one(".icon-date") else ""
        date_match = re.search(r"(\d{4})\s*-\s*(\d{1,2})", year_text)
        if not href or not title_text or not date_match or not day_text.isdigit():
            continue
        try:
            news_date = datetime(int(date_match.group(1)), int(date_match.group(2)), int(day_text)).date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        results.append(make_news_item(source, source, news_date, title_text, urljoin(URLS[source], href)))
    return results
