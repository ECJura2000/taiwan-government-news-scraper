import re
from urllib.parse import urljoin

from ....config import SFAA_LIST_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_sfaa_this_week():
    source = "社家署"
    soup = make_soup(fetch_html_resilient(URLS[source], timeout=SFAA_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("table tbody.JQ_list tr")
    if not rows:
        rows = soup.select("tbody.JQ_list tr")
    if not rows:
        raise ValueError("社家署頁面找不到 table tbody.JQ_list tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        title_td = tr.select_one('td[data-label="標題"]')
        date_td = tr.select_one('td[data-label="發布時間"]')
        if not title_td or not date_td:
            continue
        a_tag = title_td.select_one("a[href]")
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        try:
            news_date = roc_to_ad_date(clean_text(date_td.get_text(" ", strip=True)))
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        match = re.match(r"^[\[【](.*?)[\]】]\s*(.*)$", title_text)
        if match:
            department_label = "{}／{}".format(source, clean_text(match.group(1)))
            title_text = clean_text(match.group(2))
        else:
            department_label = source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
