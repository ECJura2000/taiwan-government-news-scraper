from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_oac_this_week():
    return scrape_standard_rss_this_week(
        "海委會",
        URLS["海委會"],
        department_aliases={"海洋委員會"},
    )
