from urllib.parse import urljoin

from ....config import NPA_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_npa_this_week():
    source = "警政署"
    html = fetch_html_resilient(
        URLS[source],
        timeout=NPA_TIMEOUT,
        extra_headers={"Connection": "close"},
    )

    soup = make_soup(html)
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    rows = soup.select("div.list_group div.list ul > li")
    if not rows:
        rows = soup.select("div.list ul > li")
    if not rows:
        rows = soup.select("ul > li")
    if not rows:
        raise ValueError("警政署頁面找不到可解析的新聞列表。")

    for li in rows:
        a_tag = li.select_one("a[href]")
        if not a_tag:
            continue
        info_div = a_tag.select_one("div.info")
        if info_div is None:
            continue
        title_text = a_tag.get("title", "").strip() or clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())

        date_text = ""
        category_text = ""
        department_text = ""
        for p_tag in info_div.select("p"):
            text = clean_text(p_tag.get_text(" ", strip=True))
            if text.startswith("更新日期"):
                date_text = text.replace("更新日期：", "").replace("更新日期", "").strip()
            elif text.startswith("分類"):
                category_text = text.replace("分類：", "").replace("分類", "").strip()
            elif text.startswith("發布單位"):
                department_text = text.replace("發布單位：", "").replace("發布單位", "").strip()
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
        department_label = "{}／{}".format(source, department_text) if department_text else source
        results.append(make_news_item(source, department_label, news_date, title_text, link, category=category_text))
    return results
