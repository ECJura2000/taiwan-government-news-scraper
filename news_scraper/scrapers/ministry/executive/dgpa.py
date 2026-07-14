from ....config import RSS_FEED_TIMEOUT, URLS
from ....models import make_news_item
from ....rss.parser import (
    extract_rss_item_date_fields,
    extract_rss_item_metadata_fields,
    fetch_rss_items,
    resolve_rss_news_date_with_source,
)
from ....utils.dates import get_cached_week_range

def resolve_dgpa_news_date(item):
    date_fields = extract_rss_item_date_fields(item)
    news_date, _ = resolve_rss_news_date_with_source(
        date_fields,
        allow_description_fallback=True,
        source="人事總處",
    )
    return news_date


def scrape_dgpa_this_week():
    source = "人事總處"
    start_of_week, end_of_week = get_cached_week_range()
    results = []

    for item in fetch_rss_items(URLS[source], timeout=RSS_FEED_TIMEOUT):
        date_fields = extract_rss_item_date_fields(item)
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

        fields = extract_rss_item_metadata_fields(item)
        if not fields["title"] or not fields["link"]:
            continue
        results.append(
            make_news_item(
                source,
                source,
                news_date,
                fields["title"],
                fields["link"],
                summary=fields["description"],
                date_source=date_source,
            )
        )

    return results
