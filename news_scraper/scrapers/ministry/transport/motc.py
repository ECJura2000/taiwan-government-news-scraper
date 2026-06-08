from urllib.parse import urljoin

from ....config import MOTC_LIST_TIMEOUT, URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup


def scrape_motc_this_week():
    source = "交通部"
    soup = make_soup(fetch_html(URLS[source], timeout=MOTC_LIST_TIMEOUT))
    items = soup.select("div.list_group div.list ul > li")
    if not items:
        raise ValueError("交通部頁面找不到 div.list_group div.list ul > li。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in items:
        a_tag = li.select_one("a[href]")
        if not a_tag:
            continue
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        date_text = None
        department_text = source
        for p in a_tag.select("div p"):
            text = p.get_text(" ", strip=True)
            if text.startswith("發布日期"):
                date_text = text.replace("發布日期：", "").replace("發布日期", "").strip()
            elif text.startswith("發布單位"):
                department_text = text.replace("發布單位：", "").replace("發布單位", "").strip()
        if not date_text:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue

        title_text = a_tag.get("title", "").strip()
        if not title_text:
            title_candidates = []
            for text_node in a_tag.stripped_strings:
                node_text = text_node.strip()
                if not node_text or any(
                    node_text.startswith(prefix)
                    for prefix in ["發布日期", "更新日期", "業務分類", "新聞類別", "發布單位"]
                ):
                    continue
                title_candidates.append(node_text)
            if title_candidates:
                title_text = title_candidates[-1]

        if not title_text or not (start_of_week <= news_date <= end_of_week):
            continue
        department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
