from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_motcmpb_this_week():
    source = "航港局"
    return scrape_standard_rss_this_week(source, URLS[source])
