from ....config import FDA_RSS_TIMEOUT, URLS
from ...base import scrape_standard_rss_this_week


def scrape_fda_this_week():
    return scrape_standard_rss_this_week(
        "食藥署",
        URLS["食藥署"],
        department_aliases={"衛生福利部食品藥物管理署", "食品藥物管理署", "食藥署"},
        rss_timeout=FDA_RSS_TIMEOUT,
    )
