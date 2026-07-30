import json
from dataclasses import dataclass
from datetime import datetime
from urllib.request import Request, urlopen

from .version import CURRENT_VERSION, LATEST_RELEASE_API


@dataclass(frozen=True)
class UpdateInfo:
    current_version: str
    latest_version: str
    published_at: str
    release_url: str
    update_available: bool


def version_key(value: str) -> tuple[int, int, int]:
    normalized = value.strip().lstrip("vV")
    parts = normalized.split(".")
    if len(parts) != 3 or any(not part.isdigit() for part in parts):
        raise ValueError("版本號必須是 major.minor.patch")
    return tuple(int(part) for part in parts)  # type: ignore[return-value]


def check_latest_release(timeout: int = 5) -> UpdateInfo:
    request = Request(
        LATEST_RELEASE_API,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "taiwan-government-news-scraper/{}".format(CURRENT_VERSION),
        },
    )
    with urlopen(request, timeout=timeout) as response:  # nosec B310
        data = json.loads(response.read().decode("utf-8"))
    if data.get("draft") or data.get("prerelease"):
        raise ValueError("GitHub 最新版本不是正式 Release")
    latest = str(data.get("tag_name") or "").lstrip("vV")
    version_key(latest)
    published = str(data.get("published_at") or "")
    if published:
        try:
            published = datetime.fromisoformat(published.replace("Z", "+00:00")).date().isoformat()
        except ValueError:
            pass
    return UpdateInfo(
        current_version=CURRENT_VERSION,
        latest_version=latest,
        published_at=published,
        release_url=str(data.get("html_url") or ""),
        update_available=version_key(latest) > version_key(CURRENT_VERSION),
    )
