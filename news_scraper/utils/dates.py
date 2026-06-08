import re
from datetime import datetime, timedelta

CURRENT_WEEK_RANGE = None


def get_this_week_range(force_refresh=False):
    global CURRENT_WEEK_RANGE
    if CURRENT_WEEK_RANGE is not None and not force_refresh:
        return CURRENT_WEEK_RANGE

    today = datetime.today().date()
    weekday = today.weekday()
    start_of_week = today - timedelta(days=weekday)
    if weekday == 0:
        start_of_week -= timedelta(days=7)

    end_of_week = start_of_week + timedelta(days=6)
    CURRENT_WEEK_RANGE = (start_of_week, end_of_week)
    return CURRENT_WEEK_RANGE


def get_cached_week_range():
    global CURRENT_WEEK_RANGE
    if CURRENT_WEEK_RANGE is None:
        CURRENT_WEEK_RANGE = get_this_week_range()
    return CURRENT_WEEK_RANGE


def roc_to_ad_date(roc_date_text):
    text = roc_date_text.strip()
    text = text.replace("/", "-").replace(".", "-").replace("－", "-").replace("–", "-")
    text = re.sub(r"\s+", "", text)
    parts = text.split("-")
    if len(parts) != 3:
        raise ValueError("民國日期格式無法解析：{}".format(roc_date_text))
    roc_year = int(parts[0])
    month = int(parts[1])
    day = int(parts[2])
    return datetime(roc_year + 1911, month, day).date()


def ad_to_roc_str(ad_date_str):
    dt = datetime.strptime(ad_date_str, "%Y-%m-%d").date()
    roc_year = dt.year - 1911
    return "{}/{}/{}".format(roc_year, dt.month, dt.day)


def format_ad_and_roc_date(ad_date_str):
    return "{}（民國{}）".format(ad_date_str, ad_to_roc_str(ad_date_str))


def ad_date_to_roc_compact_str(ad_date_obj):
    roc_year = ad_date_obj.year - 1911
    return "{:03d}{:02d}{:02d}".format(roc_year, ad_date_obj.month, ad_date_obj.day)


def date_str_to_ordinal(date_str):
    return datetime(int(date_str[0:4]), int(date_str[5:7]), int(date_str[8:10])).toordinal()


def parse_rss_pubdate(pub_date_text):
    if not pub_date_text:
        return None

    text = str(pub_date_text).strip()
    text = re.sub(r"\s+", " ", text)
    text = text.replace("GMT", "+0000").replace("UTC", "+0000")

    try:
        return datetime.fromisoformat(text.replace("Z", "+00:00")).date()
    except ValueError:
        pass

    for pattern in (
        "%a, %d %b %Y %H:%M:%S %z",
        "%Y%m%dT%H%M%S",
        "%Y%m%d",
        "%Y-%m-%d",
        "%Y/%m/%d",
    ):
        try:
            return datetime.strptime(text, pattern).date()
        except ValueError:
            continue

    roc_match = re.search(r"(\d{2,3}[./-]\d{1,2}[./-]\d{1,2})", text)
    if roc_match:
        try:
            return roc_to_ad_date(roc_match.group(1).replace("/", "-").replace(".", "-"))
        except Exception:
            pass

    ad_match = re.search(r"(\d{4}[./-]\d{1,2}[./-]\d{1,2})", text)
    if ad_match:
        raw = ad_match.group(1).replace("/", "-").replace(".", "-")
        try:
            return datetime.strptime(raw, "%Y-%m-%d").date()
        except ValueError:
            pass

    return None
