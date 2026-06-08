import re
from urllib.parse import urljoin

from ....config import TPS_TIMEOUT, URLS
from ....http.client import fetch_html_resilient
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, roc_to_ad_date
from ....utils.text import clean_text
from ...base import make_soup


def scrape_tps_this_week():
    source = "最高檢察署"
    html = fetch_html_resilient(URLS[source], timeout=TPS_TIMEOUT, extra_headers={"Connection": "close"})
    soup = make_soup(html)
    rows = soup.select("div.list ul li")
    row_mode = "li"
    if not rows:
        rows = soup.select("table tbody tr")
        row_mode = "tr"
    if not rows:
        rows = soup.select("table tr")
        row_mode = "tr"
    if not rows:
        rows = [
            li
            for li in soup.select("li")
            if li.select_one("a[href]") and re.search(r"\d{2,3}年\d{1,2}月\d{1,2}日|\d{2,3}[./-]\d{1,2}[./-]\d{1,2}", li.get_text(" ", strip=True))
        ]
        row_mode = "li"
    if not rows:
        raise ValueError("最高檢察署頁面找不到可解析的新聞列表。")

    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        a_tag = row.select_one("a[href]")
        if not a_tag:
            continue
        title_attr = clean_text(a_tag.get("title", ""))
        title_text = clean_text(a_tag.get_text(" ", strip=True))
        link = urljoin(URLS[source], a_tag.get("href", "").strip())
        row_text = clean_text(row.get_text(" ", strip=True))
        date_text = ""

        if row_mode == "tr":
            for td in row.find_all("td", recursive=False):
                td_text = clean_text(td.get_text(" ", strip=True))
                match = re.search(r"(\d{2,3}[./-]\d{1,2}[./-]\d{1,2})", td_text)
                if match:
                    date_text = match.group(1)
                    break
        else:
            match = re.search(r"(\d{2,3}[./-]\d{1,2}[./-]\d{1,2})", row_text)
            if match:
                date_text = match.group(1)
            else:
                match = re.search(r"(\d{2,3})年(\d{1,2})月(\d{1,2})日", row_text)
                if match:
                    date_text = "{}-{}-{}".format(match.group(1), match.group(2), match.group(3))

        if not date_text:
            combined_text = "{} {} {}".format(title_attr, title_text, row_text).strip()
            match = re.search(r"(\d{2,3}[./-]\d{1,2}[./-]\d{1,2})", combined_text)
            if match:
                date_text = match.group(1)
            else:
                match = re.search(r"(\d{2,3})年(\d{1,2})月(\d{1,2})日", combined_text)
                if match:
                    date_text = "{}-{}-{}".format(match.group(1), match.group(2), match.group(3))
        if not date_text:
            continue

        try:
            news_date = roc_to_ad_date(date_text.replace("/", "-").replace(".", "-"))
        except Exception:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue

        final_title = title_text or title_attr or row_text
        if final_title:
            results.append(make_news_item(source, source, news_date, final_title, link))
    return results
