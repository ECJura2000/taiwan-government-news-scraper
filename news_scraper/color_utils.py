import re


HEX_COLOR_PATTERN = re.compile(r"#[0-9A-Fa-f]{6}")


def normalize_hex_color(value: str) -> str | None:
    normalized = str(value).strip().upper()
    return normalized if HEX_COLOR_PATTERN.fullmatch(normalized) else None


def relative_luminance(hex_color: str) -> float:
    normalized = normalize_hex_color(hex_color)
    if normalized is None:
        raise ValueError("顏色必須是 #RRGGBB")
    color = normalized[1:]
    channels = [int(color[index : index + 2], 16) / 255 for index in (0, 2, 4)]
    linear = [
        channel / 12.92 if channel <= 0.04045 else ((channel + 0.055) / 1.055) ** 2.4
        for channel in channels
    ]
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]


def contrast_ratio(first: str, second: str) -> float:
    lighter, darker = sorted(
        (relative_luminance(first), relative_luminance(second)),
        reverse=True,
    )
    return (lighter + 0.05) / (darker + 0.05)


def readable_text_color(background: str) -> tuple[str, float]:
    black_ratio = contrast_ratio(background, "#000000")
    white_ratio = contrast_ratio(background, "#FFFFFF")
    color = "#000000" if black_ratio >= white_ratio else "#FFFFFF"
    return color, max(black_ratio, white_ratio)
