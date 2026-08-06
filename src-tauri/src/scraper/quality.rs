use super::NewsItem;
use std::collections::HashSet;

pub fn dedupe(items: impl IntoIterator<Item = NewsItem>) -> Vec<NewsItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| {
            let key = format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                item.source, item.date, item.title, item.link
            );
            seen.insert(key)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> NewsItem {
        NewsItem {
            source: "來源".into(),
            date: "2026-08-06".into(),
            department: String::new(),
            title: title.into(),
            link: "https://example.test".into(),
            category: String::new(),
            summary: String::new(),
            date_source: "published".into(),
        }
    }

    #[test]
    fn removes_exact_duplicates_and_preserves_order() {
        let result = dedupe([item("一"), item("一"), item("二")]);
        assert_eq!(
            result
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["一", "二"]
        );
    }
}
