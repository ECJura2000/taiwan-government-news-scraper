import logging
from concurrent.futures import ThreadPoolExecutor, as_completed
from urllib.parse import urlparse

from ....config import MOF_RSS_TIMEOUT, get_source_urls
from ....http.client import fetch_html_by_curl
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    parse_rss_items,
    resolve_rss_news_date_with_source,
)
from ....utils.dates import date_str_to_ordinal, get_cached_week_range

logger = logging.getLogger(__name__)
FAST_RSS_CURL_HOSTS = {"www.etax.nat.gov.tw"}


def fetch_mof_rss_items(rss_url):
    host = urlparse(rss_url).netloc.lower()
    if host in FAST_RSS_CURL_HOSTS:
        xml_text = fetch_html_by_curl(rss_url, timeout=MOF_RSS_TIMEOUT)
        return parse_rss_items(xml_text, rss_url)
    return fetch_rss_items(rss_url, timeout=MOF_RSS_TIMEOUT)


def scrape_mof_this_week():
    source = "財政部"
    rss_urls = get_source_urls(source)

    start_of_week, end_of_week = get_cached_week_range()
    seen_links = set()
    errors = []

    def fetch_and_parse_single_rss(rss_url):
        local_results = []
        local_errors = []
        try:
            items = fetch_mof_rss_items(rss_url)
            for item in items:
                date_fields = extract_rss_item_date_fields(item)
                news_date, date_source = resolve_rss_news_date_with_source(date_fields, source=source)
                if news_date is None:
                    continue
                if news_date < start_of_week:
                    continue
                if news_date > end_of_week:
                    continue

                fields = extract_rss_item_metadata_fields(item)
                if not fields["title"] or not fields["link"]:
                    continue

                title_text = fields["title"]
                description_text = ""
                author_text = fields["author"]
                if title_text in {"本部新聞", "稅務新聞", "賦稅"}:
                    description_text = fields["description"]

                department_label = source
                if "www.mof.gov.tw" in rss_url:
                    department_label = "財政部／本部新聞"
                elif author_text and author_text != source:
                    department_label = "{}／{}".format(source, author_text)

                final_title = title_text
                if description_text:
                    short_desc = description_text[:120].strip()
                    if short_desc:
                        final_title = "{}｜{}".format(title_text, short_desc)

                local_results.append(
                    make_news_item(
                        source,
                        department_label,
                        news_date,
                        final_title,
                        fields["link"],
                        summary=fields["description"],
                        date_source=date_source,
                    )
                )
        except Exception as exc:
            logger.warning("財政部 RSS 抓取失敗：%s，錯誤：%s", rss_url, exc)
            local_errors.append("{} ({})".format(exc, rss_url))
        return local_results, local_errors

    all_results = []
    with ThreadPoolExecutor(max_workers=min(2, len(rss_urls))) as executor:
        future_map = {
            executor.submit(fetch_and_parse_single_rss, rss_url): rss_url
            for rss_url in rss_urls
        }
        for future in as_completed(future_map):
            partial_results, partial_errors = future.result()
            errors.extend(partial_errors)
            for item in partial_results:
                if item["link"] in seen_links:
                    continue
                seen_links.add(item["link"])
                all_results.append(item)

    all_results.sort(
        key=lambda x: (date_str_to_ordinal(x["date"]), x["title"]),
        reverse=True,
    )

    if not all_results and errors:
        raise ValueError("；".join(errors))
    if errors:
        logger.warning("財政部部分 RSS 來源失敗：%s", "；".join(errors))
    return all_results
