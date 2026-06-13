from .errors import ValidationError


def validate_rss_items(items, url: str):
    if not isinstance(items, list) or not items:
        raise ValidationError(f"RSS 未提供 item list：{url}")
    return items
