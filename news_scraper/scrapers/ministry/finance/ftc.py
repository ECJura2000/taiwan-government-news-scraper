from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_ftc_this_week():
    return scrape_standard_rss_this_week(
        "公平會",
        URLS["公平會"],
        department_aliases={"公平交易委員會", "公平會"},
    )
