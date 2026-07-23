from datetime import datetime
from urllib.parse import urljoin

from ....config import NAER_LIST_TIMEOUT, NAER_MAX_PAGES, PAGED_SITE_WORKERS, URLS
from ....http.async_client import fetch_paginated_soups
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text

PAGE_FETCH_BATCH_SIZE = 3


def scrape_naer_this_week():
    source = "國教院"
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    seen_links = set()

    def build_list_url(page):
        if page == 1:
            return URLS[source]
        return "https://www.naer.edu.tw/PageDoc/go_page?page={}&fid=15".format(page)

    stop_scanning = False
    for batch_start in range(1, NAER_MAX_PAGES + 1, PAGE_FETCH_BATCH_SIZE):
        batch_pages = list(range(batch_start, min(NAER_MAX_PAGES + 1, batch_start + PAGE_FETCH_BATCH_SIZE)))
        page_url_pairs = [(page, build_list_url(page)) for page in batch_pages]
        page_soups = fetch_paginated_soups(
            page_url_pairs,
            max_workers=PAGED_SITE_WORKERS,
            timeout=NAER_LIST_TIMEOUT,
            extra_headers={"Connection": "close"},
        )

        for page, list_url in page_url_pairs:
            soup = page_soups.get(page)
            if soup is None:
                continue
            rows = soup.select("ul.page-list.list > li")
            if not rows:
                rows = soup.select("ul.page-list > li")
            if not rows:
                if page == 1:
                    raise ValueError("國教院頁面找不到新聞列表 ul.page-list li。")
                stop_scanning = True
                break

            page_has_data = False
            page_has_this_week_news = False
            oldest_date_in_page = None

            for li in rows:
                a_tag = li.select_one("a.txt[href]") or li.select_one("a[href]")
                date_tag = li.select_one("div.page-list-info span.date") or li.select_one("span.date")
                unit_tag = li.select_one("div.page-list-info span.unit") or li.select_one("span.unit")
                type_tag = li.select_one("div.page-list-info span.type") or li.select_one("span.type")
                if not a_tag or not date_tag:
                    continue
                page_has_data = True
                category_text = clean_text(type_tag.get_text(" ", strip=True)) if type_tag else ""
                if category_text and category_text != "新聞公告":
                    continue
                try:
                    news_date = datetime.strptime(clean_text(date_tag.get_text(" ", strip=True)), "%Y-%m-%d").date()
                except ValueError:
                    continue
                if oldest_date_in_page is None or news_date < oldest_date_in_page:
                    oldest_date_in_page = news_date
                if news_date < start_of_week or news_date > end_of_week:
                    continue
                page_has_this_week_news = True
                title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
                link = urljoin(list_url, clean_text(a_tag.get("href", "")))
                if not title_text or not link or link in seen_links:
                    continue
                seen_links.add(link)
                unit_text = clean_text(unit_tag.get_text(" ", strip=True)) if unit_tag else ""
                department_label = "{}／{}".format(source, unit_text) if unit_text and unit_text != source else source
                results.append(make_news_item(source, department_label, news_date, title_text, link))

            if not page_has_data:
                stop_scanning = True
                break
            if (not page_has_this_week_news) and oldest_date_in_page and oldest_date_in_page < start_of_week:
                stop_scanning = True
                break

        if stop_scanning:
            break
    return results
