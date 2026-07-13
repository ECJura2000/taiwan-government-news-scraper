from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_freeway_this_week():
    source = "高速公路局"
    return scrape_standard_rss_this_week(source, URLS[source])
