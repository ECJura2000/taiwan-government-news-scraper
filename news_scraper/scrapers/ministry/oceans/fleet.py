from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_cga_fleet_this_week():
    return scrape_standard_rss_this_week(
        "艦隊分署",
        URLS["艦隊分署"],
        department_aliases={"海洋委員會海巡署艦隊分署", "艦隊分署"},
    )
