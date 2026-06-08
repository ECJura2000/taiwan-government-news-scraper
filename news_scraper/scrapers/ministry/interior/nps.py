from ....config import NPS_JSON_TIMEOUT, URLS
from ....http.client import fetch_json_data
from ....models import make_news_item
from ....utils.dates import get_cached_week_range, parse_rss_pubdate
from ....utils.text import build_department_label, clean_text


def scrape_nps_this_week():
    source = "國家公園署"
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    rows = fetch_json_data(
        "https://www.nps.gov.tw/sites/www.nps.gov.tw/ch/main/parknews/list.json",
        timeout=NPS_JSON_TIMEOUT,
        extra_headers={"Connection": "close"},
    )
    if not isinstance(rows, list) or not rows:
        raise ValueError("國家公園署 JSON 列表找不到資料。")

    for row in rows:
        news_date = parse_rss_pubdate(clean_text(row.get("publish_up", "")))
        if news_date is None:
            continue
        if news_date < start_of_week:
            break
        if news_date > end_of_week:
            continue
        title_text = clean_text(row.get("title", ""))
        row_id = row.get("id")
        if not title_text or not row_id:
            continue
        department_text = ""
        unit_info = row.get("jsh_unit", {})
        if isinstance(unit_info, dict):
            department_text = clean_text(unit_info.get("name", ""))
        department_label = build_department_label(
            source,
            department_text,
            aliases={"內政部國家公園署", "國家公園署"},
        )
        link = "{}/{}".format(URLS[source].rstrip("/"), row_id)
        results.append(make_news_item(source, department_label, news_date, title_text, link))
    return results
