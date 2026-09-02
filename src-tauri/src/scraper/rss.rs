use super::{NewsItem, ScraperError};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use regex::Regex;

fn empty_item(source: &str) -> NewsItem {
    NewsItem {
        source: source.to_owned(),
        date: String::new(),
        department: source.to_owned(),
        title: String::new(),
        link: String::new(),
        category: String::new(),
        summary: String::new(),
        date_source: "published".to_owned(),
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn assign_field(item: &mut NewsItem, field: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match field {
        "title" if item.title.is_empty() => item.title = value.to_owned(),
        // RSS often places a non-permalink GUID before the canonical link.
        // Keep the GUID as a fallback, but always prefer an explicit link.
        "link" => item.link = value.to_owned(),
        "guid" if item.link.is_empty() => item.link = value.to_owned(),
        "description" | "summary" | "content" | "encoded" if item.summary.is_empty() => {
            item.summary = value.to_owned();
        }
        "pubdate" | "published" | "datetime" | "date" if item.date.is_empty() => {
            item.date = value.to_owned();
            item.date_source = "published".into();
        }
        "updated" if item.date.is_empty() => {
            item.date = value.to_owned();
            item.date_source = "updated".into();
        }
        "departmentallname" | "deptname" | "source" | "author" | "creator" | "publisher"
        | "rights"
            if item.department == item.source =>
        {
            let department = normalize_department(value);
            if !department.is_empty() && !is_department_alias(&item.source, &department) {
                item.department = department;
            }
        }
        "category" if item.category.is_empty() => item.category = value.to_owned(),
        _ => {}
    }
}

fn is_department_alias(source: &str, department: &str) -> bool {
    department == source
        || matches!(
            (source, department),
            ("國發會", "國家發展委員會")
                | ("金管會", "金融監督管理委員會")
                | ("金管會", "行政院金融監督管理委員會")
        )
        || (department.ends_with(source)
            && department.chars().count() <= source.chars().count() + 6)
}

fn finalize_item(item: &mut NewsItem) {
    item.title = item.title.split_whitespace().collect::<Vec<_>>().join(" ");
    item.department = item
        .department
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("Copyright&copy;", "Copyright &copy;");
    if let Ok(url) = url::Url::parse(item.link.trim()) {
        item.link = url.to_string();
    }
    if !item.summary.is_empty() {
        item.summary =
            super::html::clean_text(scraper::Html::parse_fragment(&item.summary).root_element())
                .chars()
                .take(4_000)
                .collect::<String>()
                .trim()
                .to_owned();
    }
    if item.source == "金管會" {
        let contact = Regex::new(r"聯絡單位\s*[:：]\s*(.*?)\s+聯絡電話\s*[:：]")
            .expect("valid FSC contact regex");
        let person_title =
            Regex::new(r"(?:副)?(?:科長|組長|主任|專員|秘書|視察|科員|技正|稽核|先生|小姐)$")
                .expect("valid FSC title regex");
        let mut units = Vec::new();
        for captures in contact.captures_iter(&item.summary) {
            let Some(raw) = captures.get(1) else {
                continue;
            };
            let mut parts = raw.as_str().split_whitespace().collect::<Vec<_>>();
            if parts.last().is_some_and(|part| person_title.is_match(part)) {
                parts.pop();
            }
            let unit = parts
                .join(" ")
                .trim_matches(|character: char| {
                    matches!(character, ' ' | '，' | ',' | '、' | ';' | '；')
                })
                .to_owned();
            if !unit.is_empty() && !units.contains(&unit) {
                units.push(unit);
            }
        }
        if !units.is_empty() {
            item.department = units.join("、");
        }
    }
}

fn normalize_department(value: &str) -> String {
    let prefix = Regex::new(
        r"^(?:版權[來來]自|版權所有|發佈單位|發布單位|提供機關|資料來源|資料來源單位|來源)[:：]\s*",
    )
    .expect("valid department prefix regex");
    let internal_code = Regex::new(r"^\d+(?:\.\d+){3,}$").expect("valid internal department regex");
    let mut normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let updated = prefix.replace(&normalized, "").trim().to_owned();
        if updated == normalized {
            break;
        }
        normalized = updated;
    }
    let compact = normalized.split_whitespace().collect::<String>();
    if internal_code.is_match(&normalized)
        || compact.starts_with("http://")
        || compact.starts_with("https://")
        || compact.starts_with("ftp://")
        || compact.to_ascii_lowercase().starts_with("www.")
    {
        return String::new();
    }
    normalized.replace(['＞', '>'], "／")
}

pub fn parse_feed(source: &str, xml: &str) -> Result<Vec<NewsItem>, ScraperError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut item_depth = 0_usize;
    let mut field = String::new();
    let mut field_value = String::new();
    let mut current = empty_item(source);
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                let name = local_name(tag.name().as_ref());
                if name == "item" || name == "entry" {
                    if item_depth == 0 {
                        current = empty_item(source);
                    }
                    item_depth += 1;
                    field.clear();
                    field_value.clear();
                } else if item_depth == 1 {
                    if name == "link" && current.link.is_empty() {
                        if let Some(attribute) = tag
                            .attributes()
                            .flatten()
                            .find(|attribute| local_name(attribute.key.as_ref()) == "href")
                        {
                            current.link = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .map_err(|error| ScraperError::ParserRegression(error.to_string()))?
                                .into_owned();
                        }
                    }
                    field = name;
                    field_value.clear();
                }
            }
            Ok(Event::Empty(tag)) if item_depth == 1 => {
                let name = local_name(tag.name().as_ref());
                if name == "link" && current.link.is_empty() {
                    if let Some(attribute) = tag
                        .attributes()
                        .flatten()
                        .find(|attribute| local_name(attribute.key.as_ref()) == "href")
                    {
                        current.link = attribute
                            .normalized_value(XmlVersion::Implicit1_0)
                            .map_err(|error| ScraperError::ParserRegression(error.to_string()))?
                            .into_owned();
                    }
                }
                field.clear();
                field_value.clear();
            }
            Ok(Event::Text(text)) if item_depth == 1 && !field.is_empty() => {
                let value = text.xml10_content().into_owned();
                field_value.push_str(&value);
            }
            Ok(Event::CData(text)) if item_depth == 1 && !field.is_empty() => {
                let value = text.xml10_content().into_owned();
                field_value.push_str(&value);
            }
            Ok(Event::GeneralRef(reference)) if item_depth == 1 && !field.is_empty() => {
                let reference = reference.xml10_content();
                let encoded = format!("&{reference};");
                let value = quick_xml::escape::unescape(&encoded)
                    .map_err(|error| ScraperError::ParserRegression(error.to_string()))?;
                field_value.push_str(&value);
            }
            Ok(Event::End(tag)) => {
                let name = local_name(tag.name().as_ref());
                if name == "item" || name == "entry" {
                    item_depth = item_depth.saturating_sub(1);
                    if item_depth == 0 && !current.title.is_empty() && !current.link.is_empty() {
                        finalize_item(&mut current);
                        items.push(current.clone());
                    }
                    field.clear();
                    field_value.clear();
                } else if item_depth == 1 {
                    assign_field(&mut current, &field, &field_value);
                    field.clear();
                    field_value.clear();
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
        let items = parse_feed("測試來源", r#"<rss><channel><item><title>標題</title><link>https://example.test/1?a=1&amp;b=2</link><pubDate>2026-08-06</pubDate></item></channel></rss>"#).unwrap();
        assert_eq!(items[0].title, "標題");
        assert_eq!(items[0].date, "2026-08-06");
        assert_eq!(items[0].link, "https://example.test/1?a=1&b=2");
    }

    #[test]
    fn canonical_link_replaces_an_earlier_non_permalink_guid() {
        let items = parse_feed(
            "航港局",
            r#"<rss><channel><item><guid isPermaLink="false">uuid-only</guid><link>https://example.test/news?id=uuid-only</link><title>新聞</title><pubDate>2026-08-06</pubDate></item></channel></rss>"#,
        )
        .unwrap();

        assert_eq!(items[0].link, "https://example.test/news?id=uuid-only");
    }

    #[test]
    fn parses_cdata_namespaces_atom_links_and_resets_items() {
        let items = parse_feed(
            "測試來源",
            r#"<feed xmlns:dc="urn:dc"><entry><title><![CDATA[第一則]]></title><link href="https://example.test/1?a=1&amp;b=2"/><dc:date>2026-08-06</dc:date><dc:creator>第一單位</dc:creator></entry><entry><title>第二則</title><link href="https://example.test/2"/><published>2026-08-05</published></entry></feed>"#,
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].link, "https://example.test/1?a=1&b=2");
        assert_eq!(items[0].department, "第一單位");
        assert_eq!(items[1].title, "第二則");
        assert_eq!(items[1].department, "測試來源");
    }

    #[test]
    fn namespaced_duplicate_fields_do_not_overwrite_primary_rss_values() {
        let items = parse_feed(
            "行政院",
            r#"<rss xmlns:dc="urn:dc"><channel><item><title><![CDATA[完整標題]]></title><link>https://example.test/1</link><description><![CDATA[完整摘要]]></description><pubDate>Thu, 06 Aug 2026 03:00:00 GMT</pubDate><dc:title>不應覆寫</dc:title><dc:description>不應覆寫摘要</dc:description><dc:date>2026-08-05</dc:date></item></channel></rss>"#,
        )
        .unwrap();
        assert_eq!(items[0].title, "完整標題");
        assert_eq!(items[0].summary, "完整摘要");
        assert_eq!(items[0].date, "Thu, 06 Aug 2026 03:00:00 GMT");
    }

    #[test]
    fn nested_attachment_items_do_not_replace_the_parent_news_item() {
        let items = parse_feed(
            "法務部",
            r#"<rss><channel><item><title>新聞標題</title><link>https://example.test/news</link><pubDate>Tue, 04 Aug 2026 06:22:28 GMT</pubDate><attachments><item><title>附件</title><link>https://example.test/file</link></item></attachments></item></channel></rss>"#,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "新聞標題");
        assert_eq!(items[0].link, "https://example.test/news");
    }

    #[test]
    fn normalizes_summary_department_and_url_metadata() {
        let items = parse_feed(
            "金管會",
            r#"<rss><channel><item><title>金融新聞</title><link>https://EXAMPLE.test/news?a=1&amp;b=2</link><description><![CDATA[<p>聯絡單位：銀行局本國銀行組 王科長 聯絡電話：(02)1234</p>]]></description><pubDate>2026-08-06</pubDate></item></channel></rss>"#,
        )
        .unwrap();
        assert_eq!(items[0].link, "https://example.test/news?a=1&b=2");
        assert_eq!(
            items[0].summary,
            "聯絡單位：銀行局本國銀行組 王科長 聯絡電話：(02)1234"
        );
        assert_eq!(items[0].department, "銀行局本國銀行組");

        let rights = parse_feed(
            "高速公路局",
            r#"<rss><channel><item><title>公路新聞</title><link>https://example.test/1</link><rights>交通部高速公路局全球資訊網 版權所有</rights><pubDate>2026-08-06</pubDate></item></channel></rss>"#,
        )
        .unwrap();
        assert_eq!(rights[0].department, "交通部高速公路局全球資訊網 版權所有");
    }
}
