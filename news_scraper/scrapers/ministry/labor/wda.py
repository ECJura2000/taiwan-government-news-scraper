from ....config import WDA_RSS_TIMEOUT, URLS
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date_with_source,
)
from ....utils.dates import get_cached_week_range
from ....utils.text import build_department_label


def scrape_wda_this_week():
    source = "勞動力發展署"
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    items = fetch_rss_items(URLS[source], timeout=WDA_RSS_TIMEOUT)
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
        department_label = build_department_label(
            source,
            fields.get("department_all_name", "") or fields["deptname"],
            aliases={"勞動部勞動力發展署", "勞動力發展署"},
        )
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
