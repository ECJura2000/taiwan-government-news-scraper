import json
from datetime import datetime, timedelta, timezone
from urllib.parse import urljoin

from ....config import TASA_GRAPHQL_TIMEOUT, URLS
from ....http.client import fetch_response
from ....models import make_news_item
from ....utils.dates import get_cached_week_range
from ....utils.text import clean_text

TASA_GRAPHQL_URL = "https://www.tasa.org.tw/graphql"
TASA_TIMEZONE = timezone(timedelta(hours=8))
TASA_ANNOUNCEMENTS_QUERY = """
query FindAnnouncements(
  $offset: Int,
  $limit: Int,
  $announcementType: [AnnouncementTypes!],
  $isIncludeEmptyEn: Boolean = true
) {
  findAnnouncements(
    offset: $offset,
    limit: $limit,
    isPublished: true,
    isDraft: false,
    isPassed: true,
    announcementType: $announcementType,
    isIncludeEmptyEn: $isIncludeEmptyEn
  ) {
    total
    items {
      id
      title {
        ZH_TW
        EN_US
      }
      announcementType
      publishAt
    }
  }
}
"""


def parse_tasa_publish_date(publish_at):
    if not publish_at:
        return None
    try:
        parsed = datetime.fromisoformat(str(publish_at).replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.date()
    return parsed.astimezone(TASA_TIMEZONE).date()


def fetch_tasa_announcements(offset=0, limit=50):
    payload = {
        "operationName": "FindAnnouncements",
        "query": TASA_ANNOUNCEMENTS_QUERY,
        "variables": {
            "offset": offset,
            "limit": limit,
            "announcementType": ["NEWS"],
            "isIncludeEmptyEn": True,
        },
    }
    response = fetch_response(
        TASA_GRAPHQL_URL,
        method="POST",
        data=json.dumps(payload),
        timeout=TASA_GRAPHQL_TIMEOUT,
        extra_headers={
            "Content-Type": "application/json",
            "Origin": "https://www.tasa.org.tw",
            "Referer": URLS["國家太空中心"],
        },
    )
    try:
        data = response.json()
    except Exception as exc:
        raise ValueError("國家太空中心 GraphQL 回應不是 JSON。") from exc
    if data.get("errors"):
        raise ValueError("國家太空中心 GraphQL 查詢失敗：{}".format(data["errors"]))
    announcements = data.get("data", {}).get("findAnnouncements")
    if not isinstance(announcements, dict):
        raise ValueError("國家太空中心 GraphQL 回應找不到 findAnnouncements。")
    return announcements


def build_tasa_item(row, news_date):
    title_info = row.get("title") if isinstance(row, dict) else {}
    if not isinstance(title_info, dict):
        title_info = {}
    title_text = clean_text(title_info.get("ZH_TW") or title_info.get("EN_US") or "")
    announcement_id = clean_text(row.get("id", ""))
    if not title_text or not announcement_id:
        return None
    link = urljoin(
        URLS["國家太空中心"],
        "/zh-TW/announcements/detail/{}".format(announcement_id),
    )
    return make_news_item("國家太空中心", "國家太空中心", news_date, title_text, link)


def scrape_tasa_this_week():
    start_of_week, end_of_week = get_cached_week_range()
    results = []
    offset = 0
    limit = 50

    while True:
        announcements = fetch_tasa_announcements(offset=offset, limit=limit)
        rows = announcements.get("items") or []
        if not rows:
            break

        reached_old_news = False
        for row in rows:
            if not isinstance(row, dict) or row.get("announcementType") != "NEWS":
                continue
            news_date = parse_tasa_publish_date(row.get("publishAt"))
            if news_date is None:
                continue
            if news_date < start_of_week:
                reached_old_news = True
                break
            if news_date > end_of_week:
                continue
            item = build_tasa_item(row, news_date)
            if item is not None:
                results.append(item)

        if reached_old_news:
            break
        offset += limit
        total = announcements.get("total")
        if isinstance(total, int) and offset >= total:
            break

    return results
