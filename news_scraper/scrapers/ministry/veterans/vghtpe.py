import re
from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS, VGHTPE_LIST_TIMEOUT
from ....http.client import fetch_html_by_curl
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import make_soup


def scrape_vghtpe_this_week():
    source = "榮總"
    soup = make_soup(fetch_html_by_curl(URLS[source], timeout=VGHTPE_LIST_TIMEOUT))
    rows = soup.select("table.stackedTable tbody tr")
    if not rows:
        rows = soup.select("table tbody tr")
    if not rows:
        raise ValueError("榮總頁面找不到新聞列表 table tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for tr in rows:
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 3:
            continue
        time_tag = tds[0].select_one("time")
        date_text = clean_text(time_tag.get_text(" ", strip=True)) if time_tag else clean_text(tds[0].get_text(" ", strip=True))
        category_text = clean_text(tds[1].get_text(" ", strip=True))
        a_tag = tds[2].select_one("a[href]")
        if not a_tag or not date_text:
            continue
        try:
            news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
        title_text = re.sub(r"^點擊前往\s*", "", title_text)
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if not title_text or not link:
            continue
        department_label = "{}／{}".format(source, category_text) if category_text and category_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
