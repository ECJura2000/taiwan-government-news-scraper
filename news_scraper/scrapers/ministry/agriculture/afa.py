from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_afa_this_week():
    return scrape_standard_rss_this_week(
        "農糧署",
        URLS["農糧署"],
        department_aliases={"農業部農糧署", "農糧署"},
    )
