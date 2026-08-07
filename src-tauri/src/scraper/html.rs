use super::{NewsItem, ScraperError};
use chrono::NaiveDate;
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

pub struct DatedListSelectors<'a> {
    pub item: &'a str,
    pub link: &'a str,
    pub title: &'a str,
    pub date: &'a str,
    pub summary: Option<&'a str>,
    pub department: Option<&'a str>,
    pub category: Option<&'a str>,
}

pub(crate) fn clean_text(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn parse_list_date(value: &str) -> Option<NaiveDate> {
    let captures = Regex::new(r"\b(\d{2,4})[-/.年](\d{1,2})[-/.月](\d{1,2})(?:日)?(?:\b|T|\s|$)")
        .ok()?
        .captures(value)?;
    let mut year: i32 = captures.get(1)?.as_str().parse().ok()?;
    if year < 1911 {
        year += 1911;
    }
    NaiveDate::from_ymd_opt(
        year,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(3)?.as_str().parse().ok()?,
    )
}

pub fn parse_dated_list(
    source: &str,
    html: &str,
    base_url: &str,
    selectors: &DatedListSelectors<'_>,
) -> Result<Vec<NewsItem>, ScraperError> {
    let document = Html::parse_document(html);
    let item_selector = Selector::parse(selectors.item).map_err(|error| {
        ScraperError::ParserRegression(format!("invalid item selector: {error}"))
    })?;
    let link_selector = Selector::parse(selectors.link).map_err(|error| {
        ScraperError::ParserRegression(format!("invalid link selector: {error}"))
    })?;
    let title_selector = Selector::parse(selectors.title).map_err(|error| {
        ScraperError::ParserRegression(format!("invalid title selector: {error}"))
    })?;
    let date_selector = Selector::parse(selectors.date).map_err(|error| {
        ScraperError::ParserRegression(format!("invalid date selector: {error}"))
    })?;
    let summary_selector = selectors
        .summary
        .map(Selector::parse)
        .transpose()
        .map_err(|error| {
            ScraperError::ParserRegression(format!("invalid summary selector: {error}"))
        })?;
    let base = Url::parse(base_url)
        .map_err(|error| ScraperError::ParserRegression(format!("invalid base URL: {error}")))?;
    let rows: Vec<_> = document.select(&item_selector).collect();
    if rows.is_empty() {
        return Err(ScraperError::ParserRegression(format!(
            "{source} page does not contain {}",
            selectors.item
        )));
    }

    let mut items = Vec::new();
    for row in rows {
        let link_element = if row.value().attr("href").is_some() {
            row
        } else if let Some(element) = row.select(&link_selector).next() {
            element
        } else {
            continue;
        };
        let Some(raw_link) = link_element.value().attr("href") else {
            continue;
        };
        let Some(title_element) = row.select(&title_selector).next() else {
            continue;
        };
        let Some(date_element) = row.select(&date_selector).next() else {
            continue;
        };
        let title = clean_text(title_element);
        let Some(date) = parse_list_date(&clean_text(date_element)) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let summary = summary_selector
            .as_ref()
            .and_then(|selector| row.select(selector).next())
            .map(clean_text)
            .unwrap_or_default();
        let selected_label = selectors
            .department
            .or(selectors.category)
            .and_then(|selector| Selector::parse(selector).ok())
            .and_then(|selector| row.select(&selector).next())
            .map(clean_text)
            .unwrap_or_default();
        let department = if selected_label.is_empty() || selected_label == source {
            source.to_owned()
        } else {
            format!("{source}／{selected_label}")
        };
        let category = selectors
            .category
            .and_then(|selector| Selector::parse(selector).ok())
            .and_then(|selector| row.select(&selector).next())
            .map(clean_text)
            .unwrap_or_default();
        let link = base
            .join(raw_link)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| raw_link.to_owned());
        items.push(NewsItem {
            source: source.to_owned(),
            date: date.to_string(),
            department,
            title,
            link,
            category,
            summary,
            date_source: "published".to_owned(),
        });
    }
    Ok(items)
}

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

    #[test]
    fn parses_roc_dated_list_with_summary() {
        let items = parse_dated_list(
            "行政院",
            r#"<ul class="news"><li><a href="/news/1"><div class="title">第一則 新聞</div><span class="date">115-08-06</span><p>摘要文字</p></a></li></ul>"#,
            "https://example.test/list",
            &DatedListSelectors {
                item: "ul.news > li",
                link: "a[href]",
                title: ".title",
                date: ".date",
                summary: Some("p"),
                department: None,
                category: None,
            },
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].date, "2026-08-06");
        assert_eq!(items[0].department, "行政院");
        assert_eq!(items[0].summary, "摘要文字");
        assert_eq!(items[0].link, "https://example.test/news/1");
    }
}
