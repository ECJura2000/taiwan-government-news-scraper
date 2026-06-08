from ....config import HPA_RSS_TIMEOUT, URLS
from ...base import scrape_standard_rss_this_week


def scrape_hpa_this_week():
    return scrape_standard_rss_this_week(
        "國健署",
        URLS["國健署"],
        department_aliases={"衛生福利部國民健康署", "國民健康署", "國健署"},
        rss_timeout=HPA_RSS_TIMEOUT,
    )
