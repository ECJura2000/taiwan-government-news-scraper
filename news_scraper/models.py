from dataclasses import asdict, dataclass
from datetime import date
from typing import TypedDict, cast


class NewsItemData(TypedDict):
    source: str
    date: str
    department: str
    title: str
    link: str


@dataclass(slots=True)
class NewsItem:
    source: str
    date: str
    department: str
    title: str
    link: str

    def to_dict(self) -> NewsItemData:
        return cast(NewsItemData, asdict(self))


def make_news_item(
    source: str,
    department: str,
    news_date: date | str,
    title: str,
    link: str,
    category: str = "",
) -> NewsItemData:
    del category
    return NewsItem(
        source=source,
        date=str(news_date),
        department=department,
        title=title,
        link=link,
    ).to_dict()
