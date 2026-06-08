from datetime import datetime
from urllib.parse import urljoin

from ....config import LIST_PAGE_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import build_department_label
from ...base import make_soup, parse_mol_data_spans


def scrape_blf_this_week():
    source = "勞動基金運用局"
    soup = make_soup(fetch_html(URLS[source], timeout=LIST_PAGE_TIMEOUT, extra_headers={"Connection": "close"}))
    rows = soup.select("div.item_listblock div.item_list2")
    if not rows:
        raise ValueError("勞動基金運用局頁面找不到 div.item_listblock div.item_list2。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        meta = parse_mol_data_spans(row.select_one("div.data"))
        date_text = meta.get("發布日期", "") or meta.get("更新日期", "")
        if not date_text:
            continue
        try:
            news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        a_tag = row.select_one("div.item_title a[href]")
        if not a_tag:
            continue
        title_text = a_tag.get_text(" ", strip=True)
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if not title_text or not link:
            continue
        department_label = build_department_label(
            source,
            meta.get("發布單位", ""),
            aliases={"勞動部勞動基金運用局", "勞動基金運用局"},
        )
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
