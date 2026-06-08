from ....config import MAC_RSS_TIMEOUT, URLS
from ...base import scrape_standard_rss_this_week


def scrape_mac_this_week():
    return scrape_standard_rss_this_week(
        "陸委會",
        URLS["陸委會"],
        department_aliases={"大陸委員會", "中華民國大陸委員會"},
        rss_timeout=MAC_RSS_TIMEOUT,
    )
