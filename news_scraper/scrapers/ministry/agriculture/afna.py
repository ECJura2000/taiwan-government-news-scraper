from urllib.parse import urljoin

from ....config import AFNA_LIST_TIMEOUT
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....source_catalog import execute_source_routes, get_source_routes
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup, parser_contract_error, scrape_standard_rss_this_week


def scrape_afna_this_week():
    source = "農業金融署"
    routes = get_source_routes(source)

    def handle(route):
        if route.kind == "rss":
            return scrape_standard_rss_this_week(source, route.url, rss_timeout=AFNA_LIST_TIMEOUT)
        html = fetch_html_resilient(
            route.url,
            timeout=AFNA_LIST_TIMEOUT,
            extra_headers={"Connection": "close"},
        )
        soup = make_soup(html)
        rows = soup.select("tbody tr")
        if not rows:
            raise parser_contract_error(source, route.url, html, "tbody tr")

        start_of_week, end_of_week = get_cached_week_range()
        results = []
        for tr in rows:
            title_td = tr.select_one('td[data-th="標題"]')
            date_td = tr.select_one('td[data-th="發布日期"]')
            if not title_td or not date_td:
                continue
            a_tag = title_td.select_one("a[href]")
            if not a_tag:
                continue
            title_text = clean_text(a_tag.get("title", "")) or clean_text(a_tag.get_text(" ", strip=True))
            date_text = clean_text(date_td.get_text(" ", strip=True))
            link = urljoin(route.url, a_tag.get("href", "").strip())
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
            results.append(make_news_item(source, source, news_date, title_text, link))
        return results

    return execute_source_routes(source, routes, handle)
