from urllib.parse import urljoin

from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....source_catalog import execute_source_routes, get_source_routes
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import fetch_html_by_selenium, make_soup, parser_contract_error


def scrape_moenv_this_week():
    source = "環境部"
    routes = get_source_routes(source)

    def handle(route):
        if route.kind == "browser":
            html = fetch_html_by_selenium(route.url, timeout=15, sleep_seconds=1)
        else:
            html = fetch_html_resilient(route.url)
        soup = make_soup(html)
        if route.id == "news-portal":
            rows = soup.select("article.idx-news-card")
        else:
            rows = soup.select("ul.list_group li")
        if not rows:
            raise parser_contract_error(source, route.url, html, "新聞列表")

        start_of_week, end_of_week = get_cached_week_range()
        results = []
        for li in rows:
            a_tag = li.select_one("a[href]")
            title_tag = li.select_one(
                ".idx-news-card__title, div.title, .title, h2, h3"
            )
            date_tag = li.select_one(
                ".idx-news-card__date, span.date, time, .date"
            )
            if not a_tag or not date_tag:
                continue
            title_text = clean_text(
                title_tag.get_text(" ", strip=True) if title_tag else a_tag.get_text(" ", strip=True)
            )
            date_text = clean_text(date_tag.get("datetime", "") or date_tag.get_text(" ", strip=True))
            try:
                news_date = roc_to_ad_date(date_text)
            except ValueError:
                continue
            if start_of_week <= news_date <= end_of_week and title_text:
                results.append(
                    make_news_item(
                        source,
                        source,
                        news_date,
                        title_text,
                        urljoin(route.url, a_tag.get("href", "").strip()),
                    )
                )
        return results

    return execute_source_routes(source, routes, handle)
