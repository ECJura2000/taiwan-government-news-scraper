from dataclasses import asdict, dataclass


@dataclass(slots=True)
class NewsItem:
    source: str
    date: str
    department: str
    title: str
    link: str

    def to_dict(self):
        return asdict(self)


def make_news_item(source, department, news_date, title, link, category=""):
    del category
    return NewsItem(
        source=source,
        date=str(news_date),
        department=department,
        title=title,
        link=link,
    ).to_dict()
