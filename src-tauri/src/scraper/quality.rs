use super::NewsItem;
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use url::Url;

const TRACKING_QUERY_KEYS: [&str; 7] = [
    "fbclid",
    "gclid",
    "utm_campaign",
    "utm_content",
    "utm_medium",
    "utm_source",
    "utm_term",
];

pub struct QualityOutput {
    pub items: Vec<NewsItem>,
    pub input_count: usize,
    pub duplicate_count: usize,
    pub invalid_count: usize,
    pub excluded_non_news_count: usize,
    pub issues: Vec<Value>,
}

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

pub fn process(items: Vec<NewsItem>) -> QualityOutput {
    let input_count = items.len();
    let mut kept = Vec::new();
    let mut seen_titles = HashSet::new();
    let mut seen_urls = HashSet::new();
    let mut duplicate_count = 0;
    let mut invalid_count = 0;
    let mut excluded_non_news_count = 0;
    let mut issues = Vec::new();

    for mut item in items {
        let invalid_reason = validate(&item);
        if let Some(reason) = invalid_reason {
            invalid_count += 1;
            issues.push(json!({"category":"invalid_item", "reason":reason, "item":item}));
            continue;
        }
        item.title = clean_text(&item.title);
        if item.title.contains("事求人機關徵才系統") {
            excluded_non_news_count += 1;
            issues.push(
                json!({"category":"excluded_non_news", "reason":"title_keyword", "item":item}),
            );
            continue;
        }
        let normalized_url = normalize_url(&item.link).expect("validated URL must normalize");
        let title_key = format!(
            "{}\u{1f}{}\u{1f}{}",
            clean_text(&item.source),
            clean_text(&item.date),
            normalize_title(&item.title)
        );
        let url_key = format!("{}\u{1f}{normalized_url}", clean_text(&item.source));
        if seen_titles.contains(&title_key) || seen_urls.contains(&url_key) {
            duplicate_count += 1;
            issues.push(
                json!({"category":"duplicate", "reason":"same_source_title_or_url", "item":item}),
            );
            continue;
        }
        seen_titles.insert(title_key);
        seen_urls.insert(url_key);
        item.link = normalized_url;
        kept.push(item);
    }

    QualityOutput {
        items: kept,
        input_count,
        duplicate_count,
        invalid_count,
        excluded_non_news_count,
        issues,
    }
}

fn validate(item: &NewsItem) -> Option<String> {
    let missing = [
        ("source", item.source.as_str()),
        ("date", item.date.as_str()),
        ("title", item.title.as_str()),
        ("link", item.link.as_str()),
    ]
    .into_iter()
    .filter_map(|(name, value)| clean_text(value).is_empty().then_some(name))
    .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Some(format!("missing_fields:{}", missing.join(",")));
    }
    if NaiveDate::parse_from_str(item.date.trim(), "%Y-%m-%d").is_err() {
        return Some("invalid_date".into());
    }
    if normalize_url(&item.link).is_none() {
        return Some("invalid_url".into());
    }
    None
}

fn normalize_url(value: &str) -> Option<String> {
    let mut url = Url::parse(clean_text(value).as_str()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    let pairs = url
        .query_pairs()
        .filter(|(key, _)| {
            !TRACKING_QUERY_KEYS
                .iter()
                .any(|tracking| key.eq_ignore_ascii_case(tracking))
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.set_fragment(None);
    Some(url.to_string())
}

pub fn dedupe_affiliated(items: impl IntoIterator<Item = NewsItem>) -> Vec<NewsItem> {
    let mut kept = Vec::new();
    let mut affiliated = Vec::new();
    let mut positions: HashMap<(String, String, String), usize> = HashMap::new();

    for item in items {
        let source = clean_text(&item.source);
        let Some(group) = affiliated_group(&source) else {
            kept.push(item);
            continue;
        };
        let date = clean_text(&item.date);
        if date.is_empty() {
            kept.push(item);
            continue;
        }
        let key = (group.to_owned(), date, normalize_title(&item.title));
        if let Some(position) = positions.get(&key).copied() {
            if preferred_score(&item) < preferred_score(&affiliated[position]) {
                affiliated[position] = item;
            }
        } else {
            positions.insert(key, affiliated.len());
            affiliated.push(item);
        }
    }

    kept.extend(affiliated);
    kept
}

fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_title(value: &str) -> String {
    clean_text(value)
        .replace('（', "(")
        .replace('）', ")")
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn affiliated_group(source: &str) -> Option<&'static str> {
    let path = affiliated_path(source);
    if let Some(parent) = path.first() {
        return Some(parent);
    }
    match source {
        "內政部" => Some("內政部"),
        "國防部" => Some("國防部"),
        "教育部" => Some("教育部"),
        "法務部" => Some("法務部"),
        "交通部" => Some("交通部"),
        "農業部" => Some("農業部"),
        "衛生福利部" => Some("衛生福利部"),
        "勞動部" => Some("勞動部"),
        "數位發展部" => Some("數位發展部"),
        "國科會" => Some("國科會"),
        "海委會" => Some("海委會"),
        "退輔會" => Some("退輔會"),
        _ => None,
    }
}

fn preferred_score(item: &NewsItem) -> (u8, u8, u8, std::cmp::Reverse<usize>) {
    let source = clean_text(&item.source);
    let priority = if affiliated_path(&source).is_empty() {
        1
    } else {
        0
    };
    let department = clean_text(&item.department);
    (
        priority,
        u8::from(department.is_empty() || department == source),
        u8::from(clean_text(&item.link).is_empty()),
        std::cmp::Reverse(normalize_title(&item.title).len()),
    )
}

pub fn affiliated_path(source: &str) -> &'static [&'static str] {
    match source {
        "國土管理署" => &["內政部", "國土管理署"],
        "國家公園署" => &["內政部", "國家公園署"],
        "國土測繪中心" => &["內政部", "國土測繪中心"],
        "警政署" => &["內政部", "警政署"],
        "消防署" => &["內政部", "消防署"],
        "中科院" => &["國防部", "中科院"],
        "國防院" => &["國防部", "國防院"],
        "國教院" => &["教育部", "國教院"],
        "矯正署" => &["法務部", "矯正署"],
        "最高檢察署" => &["法務部", "最高檢察署"],
        "觀光署" => &["交通部", "觀光署"],
        "公路局" => &["交通部", "公路局"],
        "高速公路局" => &["交通部", "高速公路局"],
        "航港局" => &["交通部", "航港局"],
        "中央氣象署" => &["交通部", "中央氣象署"],
        "農業金融署" => &["農業部", "農業金融署"],
        "農糧署" => &["農業部", "農糧署"],
        "漁業署" => &["農業部", "漁業署"],
        "農村發展及水土保持署" => &["農業部", "農村發展及水土保持署"],
        "防檢署" => &["農業部", "防檢署"],
        "農科園區" => &["農業部", "農科園區"],
        "食藥署" => &["衛生福利部", "食藥署"],
        "疾管署" => &["衛生福利部", "疾管署"],
        "國健署" => &["衛生福利部", "國健署"],
        "社家署" => &["衛生福利部", "社家署"],
        "勞動力發展署" => &["勞動部", "勞動力發展署"],
        "職業安全衛生署" => &["勞動部", "職業安全衛生署"],
        "勞動基金運用局" => &["勞動部", "勞動基金運用局"],
        "數位產業署" => &["數位發展部", "數位產業署"],
        "資通安全署" => &["數位發展部", "資通安全署"],
        "國家資通安全研究院" => &["數位發展部", "國家資通安全研究院"],
        "國家實驗研究院" => &["國科會", "國家實驗研究院"],
        "國家太空中心" => &["國科會", "國家太空中心"],
        "海巡署" => &["海委會", "海巡署"],
        "艦隊分署" => &["海委會", "海巡署", "艦隊分署"],
        "偵防分署" => &["海委會", "海巡署", "偵防分署"],
        "榮總" => &["退輔會", "榮總"],
        _ => &[],
    }
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

    #[test]
    fn affiliated_dedupe_prefers_child_agency() {
        let mut parent = item("同一則（新聞）");
        parent.source = "內政部".into();
        parent.department = "內政部".into();
        parent.link = "https://example.test/parent".into();
        let mut child = item("同一則 (新聞)");
        child.source = "消防署".into();
        child.department = "消防署".into();
        child.link = "https://example.test/child".into();

        let result = dedupe_affiliated([parent, child]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, "消防署");
    }

    #[test]
    fn affiliated_dedupe_does_not_merge_unrelated_sources() {
        let mut first = item("同一則新聞");
        first.source = "行政院".into();
        let mut second = item("同一則新聞");
        second.source = "司法院".into();

        let result = dedupe_affiliated([first, second]);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn quality_dedupes_by_title_or_url_and_removes_tracking_parameters() {
        let mut first = item("同一則新聞");
        first.link = "https://EXAMPLE.test/news?id=1&utm_source=test#part".into();
        let mut duplicate_title = item("同一則新聞");
        duplicate_title.link = "https://example.test/other".into();

        let result = process(vec![first, duplicate_title]);

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.duplicate_count, 1);
        assert_eq!(result.items[0].link, "https://example.test/news?id=1");
    }

    #[test]
    fn quality_rejects_invalid_and_non_news_items() {
        let mut invalid = item("無效");
        invalid.link = "not-a-url".into();
        let excluded = item("事求人機關徵才系統公告");

        let result = process(vec![invalid, excluded]);

        assert!(result.items.is_empty());
        assert_eq!(result.invalid_count, 1);
        assert_eq!(result.excluded_non_news_count, 1);
        assert_eq!(result.issues.len(), 2);
    }
}
