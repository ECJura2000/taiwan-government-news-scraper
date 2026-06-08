from urllib.parse import urljoin

from ....config import NFA_LIST_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_nfa_this_week():
    source = "消防署"
    soup = make_soup(fetch_html_resilient(URLS[source], timeout=NFA_LIST_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("article.pageContent.news table tbody tr")
    if not rows:
        rows = soup.select("article.pageContent table tbody tr")
    if not rows:
        rows = soup.select("table tbody tr")
    if not rows:
        raise ValueError("消防署頁面找不到新聞列表 table tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 3:
            continue
        a_tag = tds[0].find("a", href=True)
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        department_text = clean_text(tds[1].get_text(" ", strip=True))
        date_text = clean_text(tds[2].get_text(" ", strip=True))
        if not title_text or not date_text:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
