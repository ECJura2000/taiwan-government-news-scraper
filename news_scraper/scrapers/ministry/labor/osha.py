from ....config import URLS
from ...base import scrape_standard_rss_this_week


def scrape_osha_this_week():
    return scrape_standard_rss_this_week(
        "職業安全衛生署",
        URLS["職業安全衛生署"],
        department_aliases={"勞動部職業安全衛生署", "職業安全衛生署"},
    )
