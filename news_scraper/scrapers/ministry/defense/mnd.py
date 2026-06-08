from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_mnd_this_week():
    source = "國防部"
    soup = make_soup(fetch_html(URLS[source]))
    rows = soup.select("div.news_list_box a.news_list")
    if not rows:
        raise ValueError("國防部頁面找不到 div.news_list_box a.news_list。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for a_tag in rows:
        date_tag = a_tag.select_one("div.date span.en")
        category_tag = a_tag.select_one("div.category.body-2")
        title_tag = a_tag.select_one("div.title.headline-4")
        if not date_tag or not title_tag:
            continue
        date_text = clean_text(date_tag.get_text(" ", strip=True)).replace(".", "-")
        category_text = clean_text(category_tag.get_text(" ", strip=True)) if category_tag else ""
        title_text = clean_text(title_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        if not date_text or not title_text or not link:
            continue
        try:
            news_date = roc_to_ad_date(date_text)
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        department_label = "{}／{}".format(source, category_text) if category_text else source
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
