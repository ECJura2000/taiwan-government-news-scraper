from ....config import RSS_FEED_TIMEOUT, URLS
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date,
)
from ....utils.dates import get_cached_week_range


def scrape_moj_this_week():
    source = "法務部"
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    for item in fetch_rss_items(URLS[source], timeout=RSS_FEED_TIMEOUT):
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
        department_label = source
        if fields["deptname"] and fields["deptname"] != source:
            department_label = "{}／{}".format(source, fields["deptname"])
        results.append(make_news_item(source, department_label, news_date, fields["title"], fields["link"]))
    return results
