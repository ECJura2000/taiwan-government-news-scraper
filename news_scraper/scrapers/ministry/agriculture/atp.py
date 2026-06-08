from ....config import ATP_RSS_TIMEOUT, URLS
from ...base import scrape_standard_rss_this_week


def scrape_atp_this_week():
    return scrape_standard_rss_this_week(
        "農科園區",
        URLS["農科園區"],
        department_aliases={"農業科技園區", "農業部農業科技園區管理中心", "屏東農業生物技術園區", "農科園區"},
        rss_timeout=ATP_RSS_TIMEOUT,
    )
