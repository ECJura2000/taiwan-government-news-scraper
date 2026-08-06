use super::{NewsItem, ScraperError};
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn parse_feed(source: &str, xml: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut in_item = false;
    let mut field = String::new();
    let mut current = NewsItem {
        source: source.to_owned(),
        date: String::new(),
        department: String::new(),
        title: String::new(),
        link: String::new(),
        category: String::new(),
        summary: String::new(),
        date_source: "published".to_owned(),
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                let name = String::from_utf8_lossy(tag.name().as_ref()).to_ascii_lowercase();
                if name == "item" || name == "entry" {
                    in_item = true;
                    current = NewsItem {
                        source: source.to_owned(),
                        date_source: "published".to_owned(),
                        ..current.clone()
                    };
                } else if in_item {
                    field = name;
                }
            }
            Ok(Event::Text(text)) if in_item && !field.is_empty() => {
                let value = text
                    .decode()
                    .map_err(|error| ScraperError::ParserRegression(error.to_string()))?
                    .into_owned();
                match field.as_str() {
                    "title" => current.title = value,
                    "link" => current.link = value,
                    "description" | "summary" | "content" => current.summary = value,
                    "pubdate" | "published" | "updated" | "date" => current.date = value,
                    _ => {}
                }
            }
            Ok(Event::End(tag)) => {
                let name = String::from_utf8_lossy(tag.name().as_ref()).to_ascii_lowercase();
                if name == "item" || name == "entry" {
                    if !current.title.is_empty() && !current.link.is_empty() {
                        items.push(current.clone());
                    }
                    in_item = false;
                    field.clear();
                } else {
                    field.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(ScraperError::ParserRegression(error.to_string())),
            _ => {}
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss_items() {
        let items = parse_feed("測試來源", r#"<rss><channel><item><title>標題</title><link>https://example.test/1</link><pubDate>2026-08-06</pubDate></item></channel></rss>"#).unwrap();
        assert_eq!(items[0].title, "標題");
        assert_eq!(items[0].date, "2026-08-06");
    }
}
