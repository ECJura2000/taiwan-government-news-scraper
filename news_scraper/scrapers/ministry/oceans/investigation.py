from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_cga_investigation_this_week():
    return scrape_standard_rss_this_week(
        "偵防分署",
        URLS["偵防分署"],
        department_aliases={"海洋委員會海巡署偵防分署", "偵防分署"},
    )
