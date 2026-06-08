from ....config import MOA_RSS_TIMEOUT, URLS
from ....rss.parser import collect_weekly_rss_results_from_feed_entries, fetch_feedparser_entries


def scrape_moa_this_week():
    source = "農業部"
    entries = fetch_feedparser_entries(URLS[source], timeout=MOA_RSS_TIMEOUT, force_requests=True)

    def resolve_department(fields):
        dept_text = fields["deptname"] or fields["author"]
        if dept_text and dept_text != source:
            return "{}／{}".format(source, dept_text)
        return source

    return collect_weekly_rss_results_from_feed_entries(
        entries,
        source,
        department_resolver=resolve_department,
    )
