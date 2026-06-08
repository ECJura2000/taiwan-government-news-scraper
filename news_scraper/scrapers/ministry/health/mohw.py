from urllib.parse import urljoin

from ....config import MOHW_LIST_TIMEOUT, PAGED_SITE_WORKERS
from ....http.async_client import fetch_paginated_soups
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date

PAGE_FETCH_BATCH_SIZE = 3


def scrape_mohw_this_week():
    source = "衛生福利部"
    start_of_week, end_of_week = get_cached_week_range()
    results = []

    def build_list_url(page):
        if page == 1:
            return "https://www.mohw.gov.tw/lp-16-1-40.html"
        return "https://www.mohw.gov.tw/lp-16-1-{}-40.html".format(page)

    stop_scanning = False
    for batch_start in range(1, 6, PAGE_FETCH_BATCH_SIZE):
        batch_pages = list(range(batch_start, min(6, batch_start + PAGE_FETCH_BATCH_SIZE)))
        page_url_pairs = [(page, build_list_url(page)) for page in batch_pages]
        page_soups = fetch_paginated_soups(
            page_url_pairs,
            max_workers=PAGED_SITE_WORKERS,
            timeout=MOHW_LIST_TIMEOUT,
            extra_headers={"Connection": "close"},
        )

        for page, _ in page_url_pairs:
            soup = page_soups.get(page)
            if soup is None:
                continue
            rows = soup.select("section.list ul li")
            if not rows:
                if page == 1:
                    raise ValueError("衛福部頁面找不到新聞列表。")
                stop_scanning = True
                break

            page_has_data = False
            page_has_this_week_news = False
            oldest_date_in_page = None

            for li in rows:
                a_tag = li.select_one("a[href]")
                p_tag = li.select_one("p")
                time_tag = li.select_one("time")
                if not a_tag or not p_tag or not time_tag:
                    continue
                page_has_data = True

                try:
                    news_date = roc_to_ad_date(time_tag.get_text(strip=True))
                except Exception:
                    continue

                if oldest_date_in_page is None or news_date < oldest_date_in_page:
                    oldest_date_in_page = news_date
                if not (start_of_week <= news_date <= end_of_week):
                    continue

                page_has_this_week_news = True
                results.append(
                    make_news_item(
                        source,
                        source,
                        news_date,
                        p_tag.get_text(" ", strip=True),
                        urljoin("https://www.mohw.gov.tw/", a_tag.get("href", "").strip()),
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
