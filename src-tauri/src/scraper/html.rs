use super::{NewsItem, ScraperError};
use scraper::{Html, Selector};
use url::Url;

pub fn parse_link_list(
    source: &str,
    html: &str,
    base_url: Option<&str>,
    selector: &str,
) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(selector)
        .map_err(|error| ScraperError::ParserRegression(format!("invalid selector: {error}")))?;
    let base = base_url.and_then(|value| Url::parse(value).ok());
    let mut items = Vec::new();
    for element in document.select(&selector) {
        let title = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_owned();
        let Some(link) = element.value().attr("href") else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let link = match &base {
            Some(base) => base
                .join(link)
                .map(|url| url.to_string())
                .unwrap_or_else(|_| link.to_owned()),
            _ => link.to_owned(),
        };
        items.push(NewsItem {
            source: source.to_owned(),
            date: String::new(),
            department: String::new(),
            title,
            link,
            category: String::new(),
            summary: String::new(),
            date_source: "published".to_owned(),
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_resolves_links() {
        let items = parse_link_list(
            "測試來源",
            r#"<a class="news" href="/news/1">第一則新聞</a><a class="other" href="/2">忽略</a>"#,
            Some("https://example.test/list"),
            "a.news",
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].link, "https://example.test/news/1");
    }
}
