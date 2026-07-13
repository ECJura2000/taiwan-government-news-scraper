from collections import Counter
from datetime import date
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from .policy import get_quality_alert_thresholds
from .utils.text import clean_text, normalize_title_for_dedupe

TRACKING_QUERY_KEYS = {"fbclid", "gclid", "utm_campaign", "utm_content", "utm_medium", "utm_source", "utm_term"}
NON_NEWS_TITLE_KEYWORDS = ("事求人機關徵才系統",)


def build_quality_alert_reasons(summary):
    thresholds = get_quality_alert_thresholds()
    input_count = max(1, summary["input_count"])
    duplicate_ratio = summary["duplicate_count"] / input_count
    excluded_ratio = summary["excluded_non_news_count"] / input_count
    reasons = []

    if summary["invalid_count"] >= thresholds["invalid_count"]:
        reasons.append("invalid_items")
    if (
        summary["duplicate_count"] >= thresholds["duplicate_count"]
        and duplicate_ratio >= thresholds["duplicate_ratio"]
    ):
        reasons.append("duplicate_spike")
    if (
        summary["excluded_non_news_count"] >= thresholds["excluded_non_news_count"]
        and excluded_ratio >= thresholds["excluded_non_news_ratio"]
    ):
        reasons.append("non_news_spike")
    return reasons


def normalize_url(url):
    text = clean_text(url)
    parsed = urlsplit(text)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return ""
    query = urlencode(
        [(key, value) for key, value in parse_qsl(parsed.query, keep_blank_values=True) if key.lower() not in TRACKING_QUERY_KEYS],
        doseq=True,
    )
    return urlunsplit((parsed.scheme.lower(), parsed.netloc.lower(), parsed.path, query, ""))


def validate_news_item(item):
    missing_fields = [
        field_name
        for field_name in ("source", "date", "title", "link")
        if not clean_text(item.get(field_name, ""))
    ]
    if missing_fields:
        return "missing_fields:{}".format(",".join(missing_fields))
    try:
        date.fromisoformat(clean_text(item["date"]))
    except ValueError:
        return "invalid_date"
    if not normalize_url(item["link"]):
        return "invalid_url"
    return ""


def process_news_quality(news_items, selected_sources):
    kept_items = []
    issues = []
    seen_title_keys = set()
    seen_url_keys = set()
    source_counts: Counter[str] = Counter()
    duplicate_count = 0
    invalid_count = 0
    excluded_non_news_count = 0
    summary_count = 0
    date_source_counts: Counter[str] = Counter()

    for item in news_items:
        error = validate_news_item(item)
        if error:
            invalid_count += 1
            issues.append({"category": "invalid_item", "reason": error, "item": dict(item)})
            continue

        title = clean_text(item["title"])
        if any(keyword in title for keyword in NON_NEWS_TITLE_KEYWORDS):
            excluded_non_news_count += 1
            issues.append({"category": "excluded_non_news", "reason": "title_keyword", "item": dict(item)})
            continue

        normalized_url = normalize_url(item["link"])
        dedupe_key = (
            clean_text(item["source"]),
            clean_text(item["date"]),
            normalize_title_for_dedupe(title),
        )
        url_key = (clean_text(item["source"]), normalized_url)
        if dedupe_key in seen_title_keys or url_key in seen_url_keys:
            duplicate_count += 1
            issues.append({"category": "duplicate", "reason": "same_source_title_or_url", "item": dict(item)})
            continue

        seen_title_keys.add(dedupe_key)
        seen_url_keys.add(url_key)
        cleaned_item = dict(item)
        cleaned_item["link"] = normalized_url
        kept_items.append(cleaned_item)
        source_counts[cleaned_item["source"]] += 1
        if clean_text(cleaned_item.get("summary", "")):
            summary_count += 1
        date_source_counts[clean_text(cleaned_item.get("date_source", "")) or "unknown"] += 1

    for source in selected_sources:
        source_counts.setdefault(source, 0)

    summary = {
        "input_count": len(news_items),
        "output_count": len(kept_items),
        "duplicate_count": duplicate_count,
        "invalid_count": invalid_count,
        "excluded_non_news_count": excluded_non_news_count,
        "source_counts": dict(source_counts),
        "summary_count": summary_count,
        "summary_coverage_rate": round(summary_count / len(kept_items), 4) if kept_items else 0.0,
        "date_source_counts": dict(date_source_counts),
        "description_fallback_count": date_source_counts["description_fallback"],
        "issues": issues,
    }
    summary["alert_reasons"] = build_quality_alert_reasons(summary)
    return kept_items, summary
