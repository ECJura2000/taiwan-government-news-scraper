import logging
import xml.etree.ElementTree as ET

from ..config import RSS_FEED_TIMEOUT
from ..errors import ParseError
from ..external_schemas import validate_rss_items
from ..http.client import (
    fetch_html,
    fetch_html_by_curl_with_headers,
    fetch_html_plain_insecure,
    fetch_response,
    is_insecure_ssl_allowed,
)
from ..models import make_news_item
from ..monitoring import record_parser_warning
from ..utils.dates import get_cached_week_range, parse_rss_pubdate
from ..utils.text import (
    clean_text,
    get_xml_local_name,
    normalize_department_metadata_text,
    xml_child_text_by_localnames,
)

logger = logging.getLogger(__name__)

FEEDPARSER_IMPORT_ERROR: ImportError | None
try:
    import feedparser
except ImportError as exc:
    feedparser = None
    FEEDPARSER_IMPORT_ERROR = exc
else:
    FEEDPARSER_IMPORT_ERROR = None


def extract_rss_item_date_fields(item):
    pub_date_text = ""
    date_source = ""
    for local_name, candidate_source in (
        ("pubDate", "published"),
        ("date", "published"),
        ("DateTime", "published"),
        ("updated", "updated"),
    ):
        pub_date_text = xml_child_text_by_localnames(item, [local_name])
        if pub_date_text:
            date_source = candidate_source
            break
    return {
        "pub_date_text": pub_date_text,
        "date_source": date_source,
        "description": xml_child_text_by_localnames(item, ["description"]),
    }


def extract_rss_item_metadata_fields(item):
    title = xml_child_text_by_localnames(item, ["title"])
    link = xml_child_text_by_localnames(item, ["link"])
    author = normalize_department_metadata_text(
        xml_child_text_by_localnames(item, ["author", "creator", "contributor", "publisher"])
    )
    description = xml_child_text_by_localnames(item, ["description"])
    department_all_name = normalize_department_metadata_text(
        xml_child_text_by_localnames(
            item,
            ["DepartmentAllName", "departmentAllName", "departmentallname"],
        )
    )
    deptname = normalize_department_metadata_text(
        xml_child_text_by_localnames(
            item,
            ["deptname", "DepartmentAllName", "departmentAllName", "departmentallname", "source", "creator", "publisher"],
        )
    )
    dc_creator = normalize_department_metadata_text(xml_child_text_by_localnames(item, ["creator"]))
    dc_publisher = normalize_department_metadata_text(xml_child_text_by_localnames(item, ["publisher"]))
    dc_contributor = normalize_department_metadata_text(xml_child_text_by_localnames(item, ["contributor"]))
    dc_source = normalize_department_metadata_text(xml_child_text_by_localnames(item, ["source"]))
    dc_rights = normalize_department_metadata_text(xml_child_text_by_localnames(item, ["rights"]))

    if not department_all_name and dc_rights:
        department_all_name = dc_rights
    if not deptname:
        for candidate in (department_all_name, dc_rights, dc_publisher, dc_creator, dc_contributor, dc_source):
            if candidate:
                deptname = candidate
                break

    return {
        "title": title,
        "link": link,
        "author": author,
        "description": description,
        "deptname": deptname,
        "department_all_name": department_all_name,
        "dc_creator": dc_creator,
        "dc_publisher": dc_publisher,
        "dc_contributor": dc_contributor,
        "dc_source": dc_source,
        "dc_rights": dc_rights,
        "rights": dc_rights,
        "publisher": dc_publisher,
    }


def rss_item_to_fields(item):
    fields = extract_rss_item_date_fields(item)
    fields.update(extract_rss_item_metadata_fields(item))
    return fields


def extract_rss_item_core_fields(item):
    return rss_item_to_fields(item)


def parse_rss_items(xml_text, url):
    xml_text = xml_text.lstrip("\ufeff\n\r\t ")
    try:
        root = ET.fromstring(xml_text)
    except ET.ParseError as exc:
        raise ParseError("RSS XML 解析失敗：{} ({})".format(exc, url)) from exc

    channel = None
    if get_xml_local_name(root.tag) == "channel":
        channel = root
    else:
        for elem in root.iter():
            if get_xml_local_name(elem.tag) == "channel":
                channel = elem
                break

    if channel is not None:
        items = [child for child in list(channel) if get_xml_local_name(child.tag) == "item"]
        if items:
            return validate_rss_items(items, url)

    items = [elem for elem in root.iter() if get_xml_local_name(elem.tag) == "item"]
    if items:
        return validate_rss_items(items, url)

    root_name = get_xml_local_name(root.tag)
    preview = clean_text(xml_text[:200])
    raise ParseError("RSS 找不到 item 節點：{}；root={}；內容片段={}".format(url, root_name, preview))


def fetch_rss_items(url, timeout=RSS_FEED_TIMEOUT):
    rss_headers = {
        "Accept": "application/rss+xml,application/xml,text/xml;q=0.9,*/*;q=0.8",
        "Connection": "close",
    }
    errors = []

    try:
        xml_text = fetch_html(url, timeout=timeout, extra_headers=rss_headers)
        return parse_rss_items(xml_text, url)
    except Exception as exc:
        errors.append(exc)

    try:
        xml_text = fetch_html_by_curl_with_headers(
            url,
            timeout=timeout,
            extra_headers=rss_headers,
            insecure=False,
        )
        return parse_rss_items(xml_text, url)
    except Exception as exc:
        errors.append(exc)

    if is_insecure_ssl_allowed(url):
        try:
            logger.warning("安全抓取皆失敗，RSS 白名單來源最後改用 verify=False：%s", url)
            xml_text = fetch_html_plain_insecure(url, timeout=timeout, extra_headers=rss_headers)
            return parse_rss_items(xml_text, url)
        except Exception as exc:
            errors.append(exc)

    raise errors[-1]


def fetch_feedparser_entries(url, timeout=RSS_FEED_TIMEOUT, force_requests=False):
    if FEEDPARSER_IMPORT_ERROR is not None:
        raise ImportError("此 RSS 來源需要 feedparser，請先安裝：pip install feedparser")

    response = None
    content = b""
    if force_requests:
        response = fetch_response(
            url,
            method="GET",
            timeout=timeout,
            extra_headers={"Connection": "close"},
        )
        content = response.content.lstrip(b"\xef\xbb\xbf\n\r\t ")
        parsed = feedparser.parse(content)
    else:
        parsed = feedparser.parse(url)

    if getattr(parsed, "bozo", False) and not getattr(parsed, "entries", None):
        bozo_exception = getattr(parsed, "bozo_exception", None)
        raise ValueError(
            "feedparser 解析失敗：{} ({})；content-type={}；前200位元組={!r}".format(
                bozo_exception,
                url,
                response.headers.get("Content-Type", "") if response is not None else "",
                content[:200] if force_requests else b"",
            )
        )

    return parsed.entries


def extract_feedparser_entry_date_fields(entry):
    pub_date_text = ""
    date_source = ""
    for field_name, candidate_source in (
        ("published", "published"),
        ("pubDate", "published"),
        ("updated", "updated"),
        ("date", "published"),
    ):
        pub_date_text = clean_text(entry.get(field_name, ""))
        if pub_date_text:
            date_source = candidate_source
            break
    return {
        "pub_date_text": pub_date_text,
        "date_source": date_source,
        "description": clean_text(entry.get("summary", "") or entry.get("description", "")),
    }


def extract_feedparser_entry_metadata_fields(entry):
    title = clean_text(entry.get("title", ""))
    link = clean_text(entry.get("link", ""))
    author = clean_text(entry.get("author", ""))
    description = clean_text(entry.get("summary", "") or entry.get("description", ""))

    source_info = entry.get("source", {})
    if isinstance(source_info, dict):
        source_title = clean_text(source_info.get("title", ""))
    else:
        source_title = clean_text(source_info) if source_info else ""

    dc_creator = clean_text(entry.get("dc_creator", ""))
    dc_publisher = clean_text(entry.get("dc_publisher", ""))
    dc_contributor = clean_text(entry.get("dc_contributor", ""))
    dc_source = clean_text(entry.get("dc_source", ""))
    dc_rights = clean_text(entry.get("dc_rights", ""))
    rights_fallback = clean_text(entry.get("rights", ""))
    publisher_fallback = clean_text(entry.get("publisher", ""))

    if not dc_rights:
        for key, value in entry.items():
            if key.lower() in {"dc_rights", "rights"}:
                dc_rights = clean_text(value)
                if dc_rights:
                    break
    if not dc_publisher:
        for key, value in entry.items():
            if key.lower() in {"dc_publisher", "publisher"}:
                dc_publisher = clean_text(value)
                if dc_publisher:
                    break

    raw_department_candidates = [
        dc_rights,
        rights_fallback,
        entry.get("deptname", ""),
        dc_publisher,
        publisher_fallback,
        dc_source,
        source_title,
    ]
    deptname = ""
    for candidate in raw_department_candidates:
        candidate_text = clean_text(candidate)
        if candidate_text:
            deptname = candidate_text
            break

    if not author and dc_creator:
        author = dc_creator
    if not author and dc_contributor:
        author = dc_contributor
    if not deptname and dc_creator:
        deptname = dc_creator
    if not deptname and dc_contributor:
        deptname = dc_contributor

    return {
        "title": title,
        "link": link,
        "author": author,
        "description": description,
        "deptname": deptname,
        "dc_creator": dc_creator,
        "dc_publisher": dc_publisher,
        "dc_contributor": dc_contributor,
        "dc_source": dc_source,
        "dc_rights": dc_rights,
        "rights": rights_fallback,
        "publisher": publisher_fallback,
    }


def feedparser_entry_to_fields(entry):
    fields = extract_feedparser_entry_date_fields(entry)
    fields.update(extract_feedparser_entry_metadata_fields(entry))
    return fields


def resolve_rss_news_date_with_source(date_fields, allow_description_fallback=False, source=""):
    news_date = parse_rss_pubdate(date_fields.get("pub_date_text", ""))
    date_source = date_fields.get("date_source", "") or "published"
    if news_date is None and allow_description_fallback:
        news_date = parse_rss_pubdate(date_fields.get("description", ""))
        if news_date is not None:
            date_source = "description_fallback"
            record_parser_warning(
                "rss.date.description_fallback",
                date_fields.get("description", ""),
                source=source,
            )
    return news_date, date_source if news_date is not None else ""


def resolve_rss_news_date(date_fields, allow_description_fallback=False, source=""):
    news_date, _ = resolve_rss_news_date_with_source(
        date_fields,
        allow_description_fallback=allow_description_fallback,
        source=source,
    )
    return news_date


def collect_weekly_rss_results_from_feed_entries(entries, source, department_resolver=None):
    start_of_week, end_of_week = get_cached_week_range()
    results = []

    for entry in entries:
        date_fields = extract_feedparser_entry_date_fields(entry)
        news_date, date_source = resolve_rss_news_date_with_source(
            date_fields,
            allow_description_fallback=True,
            source=source,
        )
        if news_date is None:
            continue
        if news_date < start_of_week:
            continue
        if news_date > end_of_week:
            continue

        fields = extract_feedparser_entry_metadata_fields(entry)
        if not fields["title"] or not fields["link"]:
            continue

        department_label = source
        if department_resolver is not None:
            department_label = department_resolver(fields)

        results.append(
            make_news_item(
                source,
                department_label,
                news_date,
                fields["title"],
                fields["link"],
                summary=fields["description"],
                date_source=date_source,
            )
        )

    return results
