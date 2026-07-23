import re
from datetime import datetime
from urllib.parse import urljoin

from requests import RequestException

from ....config import VGHTPE_LIST_TIMEOUT, get_source_url
from ....errors import DownloadError, ParseError
from ....http.client import fetch_html_by_curl_with_headers, fetch_response
from ....models import NewsItem, make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text
from ...base import fetch_html_by_selenium, make_soup

SOURCE = "榮總"
HOME_URL = "https://www.vghtpe.gov.tw/Index.action"
CHALLENGE_MARKERS = ("cf-chl-", "challenge-platform", "just a moment", "cf-mitigated")


def is_cloudflare_challenge(html: str) -> bool:
    lowered = html.lower()
    return any(marker in lowered for marker in CHALLENGE_MARKERS)


def fetch_vghtpe_http(url: str) -> str:
    try:
        response = fetch_response(url, timeout=VGHTPE_LIST_TIMEOUT)
    except RequestException as exc:
        status_code = exc.response.status_code if exc.response is not None else None
        if status_code == 403:
            raise DownloadError("榮總 HTTP 403 Cloudflare challenge：{}".format(url)) from exc
        return fetch_html_by_curl_with_headers(url, timeout=VGHTPE_LIST_TIMEOUT, insecure=False)

    mitigation = response.headers.get("cf-mitigated", "").lower()
    if response.status_code == 403 or mitigation == "challenge":
        raise DownloadError("榮總回傳 cf-mitigated Cloudflare challenge：{}".format(url))
    return response.text


def load_vghtpe_page(url: str, wait_css: str) -> str:
    errors: list[BaseException] = []
    try:
        html = fetch_vghtpe_http(url)
        if not is_cloudflare_challenge(html):
            return html
        errors.append(DownloadError("榮總回傳 Cloudflare challenge：{}".format(url)))
    except Exception as exc:
        errors.append(exc)

    try:
        html = fetch_html_by_selenium(
            url,
            wait_css=wait_css,
            timeout=VGHTPE_LIST_TIMEOUT,
            sleep_seconds=1,
        )
        if is_cloudflare_challenge(html):
            raise DownloadError("Selenium 仍停留在 Cloudflare challenge：{}".format(url))
        return html
    except Exception as exc:
        errors.append(exc)
    raise DownloadError("榮總頁面載入失敗：{}；最後錯誤：{}".format(url, errors[-1])) from errors[-1]


def parse_vghtpe_list_html(html: str) -> list[NewsItem]:
    list_url = get_source_url(SOURCE)
    soup = make_soup(html)
    rows = soup.select("table.stackedTable tbody tr")
    if not rows:
        rows = soup.select("table tbody tr")
    if not rows:
        raise ParseError("榮總頁面找不到新聞列表 table tbody tr。")

    start_of_week, end_of_week = get_cached_week_range()
    results: list[NewsItem] = []
    for tr in rows:
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 3:
            continue
        time_tag = tds[0].select_one("time")
        date_text = (
            clean_text(time_tag.get_text(" ", strip=True))
            if time_tag
            else clean_text(tds[0].get_text(" ", strip=True))
        )
        category_text = clean_text(tds[1].get_text(" ", strip=True))
        anchor = tds[2].select_one("a[href]")
        if not anchor or not date_text:
            continue
        try:
            news_date = datetime.strptime(date_text, "%Y-%m-%d").date()
        except ValueError:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_attr = anchor.get("title", "")
        title_text = clean_text(title_attr if isinstance(title_attr, str) else "")
        title_text = title_text or clean_text(anchor.get_text(" ", strip=True))
        title_text = re.sub(r"^點擊前往\s*", "", title_text)
        href = anchor.get("href", "")
        link = urljoin(list_url, href.strip() if isinstance(href, str) else "")
        if not title_text or not link:
            continue
        department = "{}／{}".format(SOURCE, category_text) if category_text and category_text != SOURCE else SOURCE
        results.append(make_news_item(SOURCE, department, news_date, title_text, link))
    return results


def parse_vghtpe_home_html(html: str) -> list[NewsItem]:
    soup = make_soup(html)
    start_of_week, end_of_week = get_cached_week_range()
    results: list[NewsItem] = []
    seen_links: set[str] = set()
    for anchor in soup.select("a[href*='News!one.action'], a[href*='News%21one.action']"):
        href = anchor.get("href", "")
        if not isinstance(href, str) or not href.strip():
            continue
        title_attr = anchor.get("title", "")
        title = clean_text(title_attr if isinstance(title_attr, str) else "")
        title = title or clean_text(anchor.get_text(" ", strip=True))
        container = anchor.find_parent(["li", "article", "div", "tr"]) or anchor.parent
        context_text = clean_text(container.get_text(" ", strip=True)) if container else title
        date_match = re.search(r"\b(\d{4})[/-](\d{1,2})[/-](\d{1,2})\b", context_text)
        if not date_match:
            continue
        try:
            news_date = datetime.strptime("-".join(date_match.groups()), "%Y-%m-%d").date()
        except ValueError:
            continue
        if not start_of_week <= news_date <= end_of_week:
            continue
        link = urljoin(HOME_URL, href.strip())
        if not title or link in seen_links:
            continue
        seen_links.add(link)
        results.append(make_news_item(SOURCE, "{}／新聞稿".format(SOURCE), news_date, title, link))
    if not results:
        raise ParseError("榮總首頁找不到本週新聞稿。")
    return results


def scrape_vghtpe_this_week() -> list[NewsItem]:
    errors: list[BaseException] = []
    try:
        return parse_vghtpe_list_html(load_vghtpe_page(get_source_url(SOURCE), "table tbody tr"))
    except Exception as exc:
        errors.append(exc)
    try:
        return parse_vghtpe_home_html(
            load_vghtpe_page(HOME_URL, "a[href*='News!one.action'], a[href*='News%21one.action']")
        )
    except Exception as exc:
        errors.append(exc)
    raise DownloadError("榮總所有官方頁面皆無法取得；最後錯誤：{}".format(errors[-1])) from errors[-1]
