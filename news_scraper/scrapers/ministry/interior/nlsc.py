from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_nlsc_this_week():
    return scrape_standard_rss_this_week(
        "國土測繪中心",
        URLS["國土測繪中心"],
        department_aliases={"國土測繪中心全球資訊網-中文網", "內政部國土測繪中心"},
    )
