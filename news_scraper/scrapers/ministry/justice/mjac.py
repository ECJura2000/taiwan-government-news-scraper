import re
from urllib.parse import urljoin

from ....config import MJAC_LIST_TIMEOUT
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....source_catalog import execute_source_routes, get_source_routes
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup, parser_contract_error


def scrape_mjac_this_week():
    source = "矯正署"
    routes = get_source_routes(source)

    def handle(route):
        html = fetch_html_resilient(route.url, timeout=MJAC_LIST_TIMEOUT, extra_headers={"Connection": "close"})
        soup = make_soup(html)
        if route.id == "official-latest-news":
            rows = soup.select("section.lp .list li")
            if not rows:
                raise parser_contract_error(source, route.url, html, "section.lp .list li")
            return _parse_limited_latest_news(source, route.url, rows)

        rows = soup.select("table.table_list tbody tr") or soup.select("table tbody tr")
        if not rows:
            raise parser_contract_error(source, route.url, html, "可解析的新聞列表")

        start_of_week, end_of_week = get_cached_week_range()
        results = []
        for row in rows:
            a_tag = row.select_one("td a[href]")
            date_cell = row.select_one("td.date") or row.select_one('td[data-th*="日期"]')
            if not a_tag or not date_cell:
                continue
            date_text = clean_text(date_cell.get_text(" ", strip=True)).replace("/", "-").replace(".", "-")
            try:
                news_date = roc_to_ad_date(date_text)
            except Exception:
                continue
            if news_date < start_of_week:
                break
            if news_date > end_of_week:
                continue
            title_text = clean_text(a_tag.get_text(" ", strip=True))
            link = urljoin(route.url, a_tag.get("href", "").strip())
            if title_text and link:
                results.append(make_news_item(source, source, news_date, title_text, link))
        return results

    return execute_source_routes(source, routes, handle)


def _parse_limited_latest_news(source, base_url, rows):
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        a_tag = row.select_one("a[href]")
        if not a_tag:
            continue
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        title_text = re.sub(r"^\d+\s+", "", title_text)
        date_match = re.search(r"(?P<year>\d{3})年(?P<month>\d{1,2})月(?P<day>\d{1,2})日", title_text)
        if not date_match:
            continue
        news_date = roc_to_ad_date(
            "{year}-{month}-{day}".format(**date_match.groupdict())
        )
        if start_of_week <= news_date <= end_of_week:
            results.append(
                make_news_item(
                    source,
                    source,
                    news_date,
                    title_text,
                    urljoin(base_url, a_tag.get("href", "").strip()),
                )
            )
    return results
