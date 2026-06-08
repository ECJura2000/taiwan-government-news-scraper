from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_ncc_this_week():
    return scrape_standard_rss_this_week(
        "通傳會",
        URLS["通傳會"],
        department_aliases={"國家通訊傳播委員會", "通訊傳播委員會", "通傳會"},
    )
