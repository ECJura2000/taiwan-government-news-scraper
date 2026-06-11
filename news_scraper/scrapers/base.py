import ast
import re
import time
from datetime import datetime

from bs4 import BeautifulSoup

from ..config import PARSER, REQUEST_TIMEOUT, RSS_FEED_TIMEOUT
from ..http.client import fetch_html
from ..models import make_news_item
from ..monitoring import record_parser_warning
from ..rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date,
)
from ..utils.dates import get_cached_week_range, roc_to_ad_date
from ..utils.text import build_department_label, clean_text

try:
    from selenium import webdriver
    from selenium.webdriver.chrome.options import Options
    from selenium.webdriver.common.by import By
    from selenium.webdriver.support import expected_conditions as EC
    from selenium.webdriver.support.ui import WebDriverWait
except ImportError as exc:
    webdriver = None
    Options = None
    By = None
    EC = None
    WebDriverWait = None
    SELENIUM_IMPORT_ERROR = exc
else:
    SELENIUM_IMPORT_ERROR = None


def make_soup(html):
    return BeautifulSoup(html, PARSER)


def make_xml_soup(xml_text):
    return BeautifulSoup(xml_text, "xml")


def collect_weekly_results_from_ordered_rows(rows, date_extractor, item_builder):
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for row in rows:
        news_date = date_extractor(row)
        if news_date is None:
            continue
        if news_date < start_of_week:
            return results, True
        if news_date > end_of_week:
            continue
        item = item_builder(row, news_date)
        if item is not None:
            results.append(item)
    return results, False


def scrape_standard_rss_this_week(
    source,
    source_url,
    department_aliases=None,
    rss_timeout=RSS_FEED_TIMEOUT,
    department_resolver=None,
):
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    department_aliases = set(department_aliases or ())
    department_aliases.add(source)

    items = fetch_rss_items(source_url, timeout=rss_timeout)
    for item in items:
        date_fields = extract_rss_item_date_fields(item)
        news_date = resolve_rss_news_date(date_fields)
        if news_date is None:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue

        fields = extract_rss_item_metadata_fields(item)
        if not fields["title"] or not fields["link"]:
            continue

        if department_resolver is not None:
            department_label = department_resolver(fields)
        else:
            department_label = build_department_label(
                source,
                fields.get("department_all_name", "") or fields["deptname"],
                aliases=department_aliases,
            )
        results.append(make_news_item(source, department_label, news_date, fields["title"], fields["link"]))
    return results


def extract_moj_list_date(li):
    text = clean_text(li.get_text(" ", strip=True))

    ad_match = re.search(r"\b(\d{4}[/-]\d{1,2}[/-]\d{1,2})\b", text)
    if ad_match:
        raw = ad_match.group(1).replace("/", "-")
        try:
            return datetime.strptime(raw, "%Y-%m-%d").date()
        except ValueError as exc:
            record_parser_warning("extract_moj_list_date.ad", raw, error=exc, source="法務部")

    roc_match = re.search(r"\b(\d{2,3}[/-]\d{1,2}[/-]\d{1,2})\b", text)
    if roc_match:
        raw = roc_match.group(1).replace("/", "-")
        try:
            return roc_to_ad_date(raw)
        except (TypeError, ValueError) as exc:
            record_parser_warning("extract_moj_list_date.roc", raw, error=exc, source="法務部")
    return None


def extract_moj_detail_meta(detail_soup):
    news_date = None
    department_text = "法務部"
    meta_candidates = detail_soup.select("div.cp_meta, div.info, div.page_info, section.cp")
    candidate_texts = [clean_text(block.get_text(" ", strip=True)) for block in meta_candidates]
    candidate_texts.append(clean_text(detail_soup.get_text(" ", strip=True)))

    for text in candidate_texts:
        if not text:
            continue
        dept_match = re.search(r"(?:發布單位|發稿單位|單位)[:：]\s*([^\s]+(?:\s*[^\s]+){0,3})", text)
        if dept_match:
            department_text = dept_match.group(1).strip()

        ad_match = re.search(r"(\d{4}[/-]\d{1,2}[/-]\d{1,2})", text)
        if ad_match:
            raw = ad_match.group(1).replace("/", "-")
            try:
                news_date = datetime.strptime(raw, "%Y-%m-%d").date()
                break
            except ValueError as exc:
                record_parser_warning("extract_moj_detail_meta.ad", raw, error=exc, source="法務部")

        roc_match = re.search(r"(\d{2,3}[/-]\d{1,2}[/-]\d{1,2})", text)
        if roc_match:
            raw = roc_match.group(1).replace("/", "-")
            try:
                news_date = roc_to_ad_date(raw)
                break
            except (TypeError, ValueError) as exc:
                record_parser_warning("extract_moj_detail_meta.roc", raw, error=exc, source="法務部")

    return news_date, department_text


def fetch_moj_detail_result(source, title_text, link, start_of_week, end_of_week):
    detail_html = fetch_html(link)
    detail_soup = make_soup(detail_html)
    news_date, department_text = extract_moj_detail_meta(detail_soup)

    if not news_date:
        return None
    if start_of_week <= news_date <= end_of_week:
        return {
            "status": "hit",
            "item": make_news_item(source, department_text, news_date, title_text, link),
            "news_date": news_date,
            "link": link,
        }
    if news_date < start_of_week:
        return {"status": "old", "item": None, "news_date": news_date, "link": link}
    return {"status": "future", "item": None, "news_date": news_date, "link": link}


def create_selenium_driver():
    if SELENIUM_IMPORT_ERROR is not None:
        raise ImportError("需要使用 Selenium 的來源才需安裝 selenium：{}".format(SELENIUM_IMPORT_ERROR))

    chrome_options = Options()
    chrome_options.add_argument("--headless=new")
    chrome_options.add_argument("--window-size=1920,1080")
    chrome_options.add_argument("--disable-blink-features=AutomationControlled")
    chrome_options.add_argument("--no-sandbox")
    chrome_options.add_argument("--disable-dev-shm-usage")
    chrome_options.add_argument(
        "user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/135.0.0.0 Safari/537.36"
    )

    try:
        driver = webdriver.Chrome(options=chrome_options)
    except Exception:
        chrome_options = Options()
        chrome_options.add_argument("--headless")
        chrome_options.add_argument("--window-size=1920,1080")
        chrome_options.add_argument("--disable-blink-features=AutomationControlled")
        chrome_options.add_argument("--no-sandbox")
        chrome_options.add_argument("--disable-dev-shm-usage")
        chrome_options.add_argument(
            "user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
            "AppleWebKit/537.36 (KHTML, like Gecko) "
            "Chrome/135.0.0.0 Safari/537.36"
        )
        driver = webdriver.Chrome(options=chrome_options)

    driver.execute_cdp_cmd(
        "Page.addScriptToEvaluateOnNewDocument",
        {
            "source": """
                Object.defineProperty(navigator, 'webdriver', {
                    get: () => undefined
                });
            """
        },
    )
    return driver


def fetch_html_by_selenium(url, wait_id=None, timeout=REQUEST_TIMEOUT, sleep_seconds=2):
    driver = create_selenium_driver()
    try:
        driver.get(url)
        if wait_id:
            WebDriverWait(driver, timeout).until(EC.presence_of_element_located((By.ID, wait_id)))
        time.sleep(sleep_seconds)
        return driver.page_source
    finally:
        driver.quit()


def parse_mol_data_spans(data_div):
    result = {}
    if data_div is None:
        return result

    for span in data_div.select("span"):
        text = clean_text(span.get_text(" ", strip=True))
        if not text:
            continue
        if "：" in text:
            key, value = text.split("：", 1)
        elif ":" in text:
            key, value = text.split(":", 1)
        else:
            continue
        key = clean_text(key)
        value = clean_text(value)
        if key and value:
            result[key] = value

    if not result:
        whole_text = clean_text(data_div.get_text(" ", strip=True))
        for key in ["發布日期", "更新日期", "發布單位"]:
            match = re.search(r"{}[：:]\s*([^\s]+(?:\s*[^\s]+)*)".format(re.escape(key)), whole_text)
            if match:
                result[key] = clean_text(match.group(1))
    return result


def extract_js_object_literal(js_text, variable_name):
    pattern = r"var\s+{}\s*=\s*(.*?);\s*(?:var\s+\w+\s*=|$)".format(re.escape(variable_name))
    match = re.search(pattern, js_text, flags=re.S)
    if not match:
        raise ValueError("找不到 JS 變數：{}".format(variable_name))
    return ast.literal_eval(match.group(1).strip())
