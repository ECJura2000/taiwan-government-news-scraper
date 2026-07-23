from urllib.parse import urljoin

from ....config import LIST_PAGE_TIMEOUT, PAGED_SITE_WORKERS, URLS
from ....http.async_client import fetch_paginated_soups
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import parse_mol_data_spans

PAGE_FETCH_BATCH_SIZE = 3


def scrape_mol_this_week():
    source = "勞動部"
    start_of_week, end_of_week = get_cached_week_range()
    results = []

    def build_list_url(page):
        if page == 1:
            return URLS[source]
        return "https://www.mol.gov.tw/1607/1632/1633/?page={}".format(page)

    stop_scanning = False
    for batch_start in range(1, 11, PAGE_FETCH_BATCH_SIZE):
        batch_pages = list(range(batch_start, min(11, batch_start + PAGE_FETCH_BATCH_SIZE)))
        page_url_pairs = [(page, build_list_url(page)) for page in batch_pages]
        page_soups = fetch_paginated_soups(
            page_url_pairs,
            max_workers=PAGED_SITE_WORKERS,
            timeout=LIST_PAGE_TIMEOUT,
        )

        for page, list_url in page_url_pairs:
            soup = page_soups.get(page)
            if soup is None:
                continue
            rows = soup.select("div.item_listblock div.item_list2")
            if not rows:
                if page == 1:
                    raise ValueError("勞動部頁面找不到 div.item_listblock div.item_list2。")
                stop_scanning = True
                break

            page_has_data = False
            page_has_this_week_news = False
            oldest_date_in_page = None

            for row in rows:
                a_tag = row.select_one("h3 a[href]")
                if not a_tag:
                    continue
                page_has_data = True
                meta = parse_mol_data_spans(row.select_one("div.data"))
                date_text = meta.get("發布日期", "") or meta.get("更新日期", "")
                department_text = meta.get("發布單位", "") or source
                if not date_text:
                    continue
                try:
                    from datetime import datetime

                    news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
                except ValueError:
                    continue
                if oldest_date_in_page is None or news_date < oldest_date_in_page:
                    oldest_date_in_page = news_date
                if not (start_of_week <= news_date <= end_of_week):
                    continue
                page_has_this_week_news = True
                department_label = "{}／{}".format(source, department_text) if department_text and department_text != source else source
                results.append(
                    make_news_item(
                        source,
                        department_label,
                        news_date,
                        a_tag.get_text(" ", strip=True),
                        urljoin(list_url, clean_text(a_tag.get("href", ""))),
                    )
                )

            if not page_has_data:
                stop_scanning = True
                break
            if (not page_has_this_week_news) and oldest_date_in_page and oldest_date_in_page < start_of_week:
                stop_scanning = True
                break

        if stop_scanning:
            break

    return results
