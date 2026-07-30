import re
from urllib.parse import urljoin

from ....config import SFAA_LIST_TIMEOUT
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....source_catalog import execute_source_routes, get_source_routes
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup, parser_contract_error


def scrape_sfaa_this_week():
    source = "社家署"
    routes = get_source_routes(source)

    def handle(route):
        html = fetch_html_resilient(
            route.url,
            timeout=SFAA_LIST_TIMEOUT,
            extra_headers={"Connection": "close"},
        )
        soup = make_soup(html)
        if route.id == "mohw-source-mirror":
            rows = soup.select("section.list ul li")
            if not rows:
                raise parser_contract_error(source, route.url, html, "section.list ul li")
            return _parse_mohw_source_mirror(source, route.url, rows)

        rows = soup.select("table tbody.JQ_list tr") or soup.select("tbody.JQ_list tr")
        if not rows:
            raise parser_contract_error(source, route.url, html, "table tbody.JQ_list tr")

        start_of_week, end_of_week = get_cached_week_range()
        results = []
        for tr in rows:
            title_td = tr.select_one('td[data-label="標題"]')
            date_td = tr.select_one('td[data-label="發布時間"]')
            if not title_td or not date_td:
                continue
            a_tag = title_td.select_one("a[href]")
            if not a_tag:
                continue
            title_text = clean_text(a_tag.get_text(" ", strip=True))
            link = urljoin(route.url, a_tag.get("href", "").strip())
            try:
                news_date = roc_to_ad_date(clean_text(date_td.get_text(" ", strip=True)))
            except Exception:
                continue
            if news_date < start_of_week:
                break
            if news_date > end_of_week:
                continue
            match = re.match(r"^[\[【](.*?)[\]】]\s*(.*)$", title_text)
            if match:
                department_label = "{}／{}".format(source, clean_text(match.group(1)))
                title_text = clean_text(match.group(2))
            else:
                department_label = source
            results.append(make_news_item(source, department_label, news_date, title_text, link))
        return results

    return execute_source_routes(source, routes, handle)


def _parse_mohw_source_mirror(source, base_url, rows):
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        a_tag = row.select_one("a[href]")
        title_tag = row.select_one("p")
        date_tag = row.select_one("time")
        if not a_tag or not title_tag or not date_tag:
            continue
        try:
            news_date = roc_to_ad_date(clean_text(date_tag.get_text(" ", strip=True)))
        except Exception:
            continue
        if not (start_of_week <= news_date <= end_of_week):
            continue
        link = urljoin(base_url, a_tag.get("href", "").strip())
        detail_html = fetch_html_resilient(link, timeout=SFAA_LIST_TIMEOUT)
        detail_text = clean_text(make_soup(detail_html).get_text(" ", strip=True))
        if not re.search(r"資料來源\s*[：:]\s*(?:社會及家庭署|社家署)", detail_text):
            continue
        results.append(
            make_news_item(
                source,
                source,
                news_date,
                clean_text(title_tag.get_text(" ", strip=True)),
                link,
            )
        )
    return results
