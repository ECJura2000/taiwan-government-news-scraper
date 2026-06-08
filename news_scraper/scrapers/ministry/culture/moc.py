from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_moc_this_week():
    source = "文化部"
    return scrape_standard_rss_this_week(source, URLS[source])
