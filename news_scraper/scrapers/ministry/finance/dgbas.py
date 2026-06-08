from ....config import RSS_FEED_TIMEOUT, URLS
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date,
)
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text


def scrape_dgbas_this_week():
    source = "主計總處"
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    items = fetch_rss_items(URLS[source], timeout=RSS_FEED_TIMEOUT)

    for item in items:
        date_fields = extract_rss_item_date_fields(item)
        news_date = resolve_rss_news_date(date_fields, allow_description_fallback=True)
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
        deptname = clean_text(fields["deptname"])
        if deptname and deptname != source:
            department_label = "{}／{}".format(source, deptname)
        results.append(make_news_item(source, department_label, news_date, fields["title"], fields["link"]))
    return results
