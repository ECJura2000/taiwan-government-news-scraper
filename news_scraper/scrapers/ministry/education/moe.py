from urllib.parse import urljoin

from ....config import LIST_PAGE_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_moe_this_week():
    source = "教育部"
    html = fetch_html_resilient(
        URLS[source],
        timeout=LIST_PAGE_TIMEOUT,
        extra_headers={"Connection": "close"},
    )

    soup = make_soup(html)
    rows = soup.select("table#ContentPlaceHolder1_gvIndex tr.question_tr")
    if not rows:
        rows = soup.select("#ContentPlaceHolder1_gvIndex tr.question_tr")
    if not rows:
        rows = soup.select("tr.question_tr")
    if not rows:
        raise ValueError("教育部頁面找不到新聞列表 tr.question_tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 3:
            continue
        date_text = clean_text(tds[0].get_text(" ", strip=True))
        unit_text = clean_text(tds[2].get_text(" ", strip=True))
        a_tag = tds[1].find("a", href=True)
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        if not date_text or not title_text:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        department_label = "{}／{}".format(source, unit_text) if unit_text and unit_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, urljoin(URLS[source], a_tag.get("href", "").strip())))
    return results
