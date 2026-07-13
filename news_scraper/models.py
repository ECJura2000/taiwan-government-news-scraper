import html
import re
from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from datetime import date
from typing import Any, TypedDict, cast

SUMMARY_TAG_RE = re.compile(r"<[^>]+>")


def normalize_news_summary(value: str, max_length: int = 4000) -> str:
    text = html.unescape(str(value or ""))
    text = SUMMARY_TAG_RE.sub(" ", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text[:max_length].rstrip()


class NewsItemData(TypedDict):
    source: str
    date: str
    department: str
    title: str
    link: str
    category: str
    summary: str
    date_source: str


@dataclass(slots=True, eq=False)
class NewsItem(Mapping[str, str]):
    source: str
    date: str
    department: str
    title: str
    link: str
    category: str = ""
    summary: str = ""
    date_source: str = "published"

    def __getitem__(self, key: str) -> str:
        try:
            value = getattr(self, key)
        except AttributeError as exc:
            raise KeyError(key) from exc
        return cast(str, value)

    def __iter__(self) -> Iterator[str]:
        return iter(("source", "date", "department", "title", "link", "category", "summary", "date_source"))

    def __len__(self) -> int:
        return 8

    def get(self, key: str, default: Any = None) -> Any:
        return getattr(self, key, default)

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Mapping):
            other_values = dict(other)
            if any(self.get(key) != value for key, value in other_values.items()):
                return False
            ignored_defaults = {"category": "", "summary": "", "date_source": "published"}
            return all(
                key in other_values or value == ignored_defaults.get(key)
                for key, value in dict(self).items()
            )
        return NotImplemented

    def to_dict(self) -> NewsItemData:
        return cast(NewsItemData, dict(self))


def make_news_item(
    source: str,
    department: str,
    news_date: date | str,
    title: str,
    link: str,
    category: str = "",
    summary: str = "",
    date_source: str = "published",
) -> NewsItem:
    return NewsItem(
        source=source,
        date=str(news_date),
        department=department,
        title=title,
        link=link,
        category=category,
        summary=normalize_news_summary(summary),
        date_source=date_source,
    )
