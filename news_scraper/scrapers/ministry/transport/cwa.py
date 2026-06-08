from datetime import datetime
from urllib.parse import urljoin

from ....config import CWA_JS_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import extract_js_object_literal


def scrape_cwa_this_week():
    source = "中央氣象署"
    js_text = fetch_html(
        "https://www.cwa.gov.tw/Data/js/service/NewsHot.js?",
        timeout=CWA_JS_TIMEOUT,
        extra_headers={"Connection": "close"},
    )
    newsbb = extract_js_object_literal(js_text, "Newsbb")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for item in newsbb.get("C", []):
        date_text = clean_text(item.get("record_date", ""))
        title_text = clean_text(item.get("title", ""))
        link = urljoin(URLS[source], clean_text(item.get("link", "")))
        if not date_text or not title_text or not link or link.endswith("#"):
            continue
        try:
            news_date = datetime.strptime(date_text[:10], "%Y/%m/%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        results.append(make_news_item(source, source, news_date, title_text, link))
    return results
