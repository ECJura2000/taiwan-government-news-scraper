from ....config import RSS_FEED_TIMEOUT, URLS
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date,
)
from ....utils.dates import get_cached_week_range


def scrape_ndc_this_week():
    source = "國發會"
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
        if fields["title"] and fields["link"]:
            results.append(make_news_item(source, source, news_date, fields["title"], fields["link"]))
    return results
