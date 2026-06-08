from ....config import RSS_FEED_TIMEOUT, URLS
from ....rss.parser import collect_weekly_rss_results_from_feed_entries, fetch_feedparser_entries


def scrape_motcmpb_this_week():
    source = "航港局"
    entries = fetch_feedparser_entries(URLS[source], timeout=RSS_FEED_TIMEOUT, force_requests=True)
    return collect_weekly_rss_results_from_feed_entries(entries, source)
