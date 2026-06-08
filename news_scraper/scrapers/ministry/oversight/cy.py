from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_cy_this_week():
    return scrape_standard_rss_this_week(
        "監察院",
        URLS["監察院"],
        department_aliases={"監察院全球資訊網", "監察院全球資訊網中文版院新聞稿"},
    )
