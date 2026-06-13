from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from datetime import date
from typing import Any, TypedDict, cast


class NewsItemData(TypedDict):
    source: str
    date: str
    department: str
    title: str
    link: str
    category: str


@dataclass(slots=True, eq=False)
class NewsItem(Mapping[str, str]):
    source: str
    date: str
    department: str
    title: str
    link: str
    category: str = ""

    def __getitem__(self, key: str) -> str:
        try:
            value = getattr(self, key)
        except AttributeError as exc:
            raise KeyError(key) from exc
        return cast(str, value)

    def __iter__(self) -> Iterator[str]:
        fields = ("source", "date", "department", "title", "link")
        return iter(fields + (("category",) if self.category else ()))

    def __len__(self) -> int:
        return 6 if self.category else 5

    def get(self, key: str, default: Any = None) -> Any:
        return getattr(self, key, default)

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Mapping):
            return dict(self) == dict(other)
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
) -> NewsItem:
    return NewsItem(
        source=source,
        date=str(news_date),
        department=department,
        title=title,
        link=link,
        category=category,
    )
