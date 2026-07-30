from urllib.parse import urljoin

from ....config import PCC_TIMEOUT
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....source_catalog import execute_source_routes, get_source_routes
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ...base import make_soup, parser_contract_error, scrape_standard_rss_this_week


def scrape_pcc_this_week():
    source = "工程會"
    routes = get_source_routes(source)

    def handle(route):
        if route.kind == "rss":
            return scrape_standard_rss_this_week(source, route.url, rss_timeout=PCC_TIMEOUT)
        html = fetch_html_resilient(
            route.url,
            timeout=PCC_TIMEOUT,
            extra_headers={"Connection": "close"},
        )
        soup = make_soup(html)
        rows = soup.select("div.tableArea tr.trPos")
        if not rows:
            raise parser_contract_error(source, route.url, html, "div.tableArea tr.trPos")

        start_of_week, end_of_week = get_cached_week_range()
        results = []
        for tr in rows:
            title_td = tr.select_one('td[data-th="名稱"]')
            date_td = tr.select_one('td[data-th="發布日期"]')
            if not title_td or not date_td:
                continue
            a_tag = title_td.select_one("a[href]")
            if not a_tag:
                continue
            date_tag = date_td.select_one("span")
            date_text = date_tag.get_text(strip=True) if date_tag else date_td.get_text(" ", strip=True)
            try:
                news_date = roc_to_ad_date(date_text)
            except Exception:
                continue
            if start_of_week <= news_date <= end_of_week:
                results.append(
                    make_news_item(
                        source,
                        source,
                        news_date,
                        a_tag.get_text(" ", strip=True),
                        urljoin(route.url, a_tag.get("href", "").strip()),
                    )
                )
        return results

    return execute_source_routes(source, routes, handle)
