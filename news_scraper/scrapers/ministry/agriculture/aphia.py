import re
from urllib.parse import urljoin

from ....config import APHIA_RSS_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_aphia_this_week():
    source = "防檢署"
    soup = make_soup(fetch_html(URLS[source], timeout=APHIA_RSS_TIMEOUT))
    anchors = soup.select('a[href*="theme_data.php?theme=NewInfoListWS"]')
    if not anchors:
        raise ValueError("防檢署頁面找不到新聞連結。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for a_tag in anchors:
        row_text = clean_text(a_tag.get_text(" ", strip=True))
        href = clean_text(a_tag.get("href", ""))
        if not row_text or not href:
            continue
        match = re.match(r"^(\d{2,3}-\d{1,2}-\d{1,2})\s+(.+)$", row_text)
        if not match:
            continue
        try:
            news_date = roc_to_ad_date(match.group(1))
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_text = clean_text(match.group(2))
        link = urljoin(URLS[source], href)
        if title_text and link:
            results.append(make_news_item(source, source, news_date, title_text, link))
    return results
