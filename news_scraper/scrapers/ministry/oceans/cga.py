from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_cga_this_week():
    return scrape_standard_rss_this_week(
        "海巡署",
        URLS["海巡署"],
        department_aliases={"海洋委員會海巡署", "海巡署"},
    )
