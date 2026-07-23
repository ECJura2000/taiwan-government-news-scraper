from typing import Any

from ..config import AFFILIATED_GROUPS
from .text import clean_text, normalize_title_for_dedupe


def build_affiliated_group_lookup():
    source_to_group = {}
    group_priority = {}
    for group_name, config in AFFILIATED_GROUPS.items():
        priority_map = config.get("priority", {})
        for member in config.get("members", []):
            source_to_group[member] = group_name
            group_priority[member] = priority_map.get(member, 99)
    return source_to_group, group_priority


AFFILIATED_SOURCE_TO_GROUP, AFFILIATED_SOURCE_PRIORITY = build_affiliated_group_lookup()


def choose_preferred_affiliated_item(left, right):
    def score(item):
        source = clean_text(item.get("source", ""))
        department = clean_text(item.get("department", ""))
        link = clean_text(item.get("link", ""))
        return (
            AFFILIATED_SOURCE_PRIORITY.get(source, 99),
            0 if department and department != source else 1,
            0 if link else 1,
            -len(normalize_title_for_dedupe(item.get("title", ""))),
        )

    return left if score(left) <= score(right) else right


def dedupe_affiliated_news(news_items):
    kept_items: list[Any] = []
    affiliated_items: dict[tuple[str, str, str], Any] = {}

    for item in news_items:
        source = clean_text(item.get("source", ""))
        group_name = AFFILIATED_SOURCE_TO_GROUP.get(source, "")
        if not group_name:
            kept_items.append(item)
            continue

        dedupe_key = (
            group_name,
            clean_text(item.get("date", "")),
            normalize_title_for_dedupe(item.get("title", "")),
        )
        if not dedupe_key[0] or not dedupe_key[1]:
            kept_items.append(item)
            continue

        existing = affiliated_items.get(dedupe_key)
        if existing is None:
            affiliated_items[dedupe_key] = item
        else:
            affiliated_items[dedupe_key] = choose_preferred_affiliated_item(existing, item)

    kept_items.extend(affiliated_items.values())
    return kept_items
