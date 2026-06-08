from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_cip_this_week():
    return scrape_standard_rss_this_week(
        "原民會",
        URLS["原民會"],
        department_aliases={"原住民族委員會"},
    )
