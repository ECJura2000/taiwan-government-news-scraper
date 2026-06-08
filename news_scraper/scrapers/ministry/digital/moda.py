import json
from datetime import datetime
from urllib.parse import urljoin

from ....config import URLS
from ....http.client import fetch_html, fetch_html_plain_insecure
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, parse_rss_pubdate
from ...base import make_soup


def scrape_moda_list_this_week(source):
    soup = make_soup(fetch_html(URLS[source]))
    container = soup.select_one("ul#ListTable")
    if container is None:
        raise ValueError("{} 頁面找不到 ul#ListTable。".format(source))

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for li in container.select("li"):
        a_tag = li.select_one("a[href]")
        date_tag = li.select_one(".listDate")
        dept_tag = li.select_one(".listUnit")
        title_tag = li.select_one(".title5") or a_tag
        if not a_tag or not date_tag or not title_tag:
            continue
        try:
            news_date = datetime.strptime(date_tag.get_text(strip=True), "%Y-%m-%d").date()
        except ValueError:
            continue
        if not (start_of_week <= news_date <= end_of_week):
            continue

        dept_text = dept_tag.get_text(" ", strip=True) if dept_tag else source
        if dept_text and dept_text != source:
            dept_text = "{}／{}".format(source, dept_text)
        else:
            dept_text = source
        results.append(
            make_news_item(
                source,
                dept_text,
                news_date,
                title_tag.get_text(" ", strip=True),
                urljoin(URLS[source], a_tag.get("href", "").strip()),
            )
        )
    return results


def scrape_moda_this_week():
    return scrape_moda_list_this_week("數位發展部")


def scrape_adi_this_week():
    return scrape_moda_list_this_week("數位產業署")


def scrape_acs_this_week():
    return scrape_moda_list_this_week("資通安全署")


def scrape_nics_this_week():
    source = "國家資通安全研究院"
    soup = make_soup(fetch_html_plain_insecure(URLS[source]))
    data_tag = soup.select_one("script#__NEXT_DATA__")
    if data_tag is None or not data_tag.string:
        raise ValueError("國家資通安全研究院頁面找不到 __NEXT_DATA__。")

    try:
        data = json.loads(data_tag.string)
        items = data["props"]["pageProps"]["data"]["content"][0]["data"]["item"]
    except (KeyError, IndexError, TypeError, json.JSONDecodeError) as exc:
        raise ValueError("國家資通安全研究院新聞資料解析失敗。") from exc

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for item in items:
        news_date = parse_rss_pubdate(item.get("date"))
        if news_date is None:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue

        title = (item.get("label") or "").strip()
        link = (item.get("link") or "").strip()
        if not title or not link:
            continue

        results.append(
            make_news_item(
                source,
                source,
                news_date,
                title,
                urljoin(URLS[source], link),
            )
        )
    return results
