use super::{
    catalog::{ItemFilters, SourceDefinition, SourceRoute},
    rss::FeedRecord,
    NewsItem,
};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn normalized(value: &str) -> String {
    value.split_whitespace().collect::<String>().to_lowercase()
}

fn matches_label(value: &str, labels: &[String]) -> bool {
    labels
        .iter()
        .any(|label| normalized(value) == normalized(label))
}

fn rejection(record: &FeedRecord, policy: &ItemFilters) -> Option<&'static str> {
    if !policy.publisher_allowlist.is_empty() || !policy.publisher_prefixes.is_empty() {
        let matches_publisher = |publisher: &str| {
            matches_label(publisher, &policy.publisher_allowlist)
                || policy.publisher_prefixes.iter().any(|prefix| {
                    let publisher = normalized(publisher);
                    let prefix = normalized(prefix);
                    publisher.strip_prefix(&prefix).is_some_and(|suffix| {
                        suffix.is_empty()
                            || suffix.starts_with(['（', '(', '／', '/', '：', ':', '-'])
                    })
                })
        };
        match record.publisher.as_deref() {
            None => return Some("publisher_not_proven"),
            Some(publisher) if !matches_publisher(publisher) => return Some("different_publisher"),
            _ => {}
        }
    }
    if !policy.include_categories.is_empty()
        && !record
            .categories
            .iter()
            .any(|category| matches_label(category, &policy.include_categories))
    {
        return Some("category_not_in_scope");
    }
    if record
        .categories
        .iter()
        .any(|category| matches_label(category, &policy.exclude_categories))
    {
        return Some("excluded_category");
    }
    if policy
        .exclude_title_prefixes
        .iter()
        .any(|prefix| record.item.title.starts_with(prefix))
    {
        return Some("excluded_title_prefix");
    }
    if policy
        .exclude_title_patterns
        .iter()
        .any(|pattern| Regex::new(pattern).is_ok_and(|regex| regex.is_match(&record.item.title)))
    {
        return Some("excluded_title_pattern");
    }
    if policy.exclude_administrative {
        const ADMIN: &[&str] = &[
            "徵才",
            "徵才公告",
            "人員招募",
            "職缺公告",
            "志工招募",
            "採購",
            "採購公告",
            "招標",
            "招標公告",
            "決標",
            "決標公告",
        ];
        if record.categories.iter().any(|category| {
            ADMIN
                .iter()
                .any(|label| normalized(category) == normalized(label))
        }) {
            return Some("administrative_category");
        }
        static TITLE: OnceLock<Regex> = OnceLock::new();
        let regex = TITLE.get_or_init(||Regex::new(r"^(?:【|\[)?(?:徵才|招標|採購|決標)(?:公告|資訊|訊息|】|\]|[：:、\s])|(?:標售案|採購案)(?:公告(?:與須知)?|招標公告|決標公告)$|(?:職缺公告|徵才公告|採購公告|招標公告|決標公告|專案志工招募)[）)]?$|事求人機關徵才系統").expect("valid administrative title rule"));
        if regex.is_match(&record.item.title) {
            return Some("administrative_title");
        }
    }
    None
}

pub fn apply(
    definition: &SourceDefinition,
    route: &SourceRoute,
    records: Vec<FeedRecord>,
) -> (Vec<NewsItem>, Vec<Value>) {
    let (records, decisions) = apply_records(definition, route, records);
    (
        records.into_iter().map(|record| record.item).collect(),
        decisions,
    )
}

/// Preserve explicit publisher/category evidence for subsequent route merging.
pub fn apply_records(
    definition: &SourceDefinition,
    route: &SourceRoute,
    records: Vec<FeedRecord>,
) -> (Vec<FeedRecord>, Vec<Value>) {
    let mut items = Vec::new();
    let mut decisions = Vec::new();
    for record in records {
        let reason = rejection(&record, &definition.filters)
            .or_else(|| rejection(&record, &route.filters))
            .or_else(|| {
                route
                    .selectors
                    .as_ref()
                    .filter(|selectors| {
                        selectors
                            .exclude_title_prefixes
                            .iter()
                            .any(|prefix| record.item.title.starts_with(prefix))
                    })
                    .map(|_| "excluded_title_prefix")
            });
        if let Some(reason) = reason {
            decisions.push(json!({"source":definition.name,"route_id":route.id,"reason":reason,"title":record.item.title,"link":record.item.link,"date":record.item.date,"publisher":record.publisher,"categories":record.categories}));
        } else {
            items.push(record);
        }
    }
    (items, decisions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(title: &str, categories: &[&str], publisher: Option<&str>) -> FeedRecord {
        FeedRecord {
            item: NewsItem {
                source: "子館".into(),
                date: "2026-10-05".into(),
                department: "子館".into(),
                title: title.into(),
                link: "https://official.test/article".into(),
                category: categories.first().unwrap_or(&"").to_string(),
                summary: String::new(),
                full_text: String::new(),
                date_source: "published".into(),
            },
            publisher: publisher.map(str::to_owned),
            categories: categories.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn administrative_categories_and_precise_titles_do_not_remove_arts_calls() {
        let policy = ItemFilters::default();
        for title in [
            "藝術家駐村甄選公告",
            "展覽徵件",
            "新採購政策協助文化產業",
            "志工服務成果發表",
            "活動報名公告",
            "115年度委託專業服務成果展開幕",
        ] {
            assert!(rejection(&record(title, &["活動"], None), &policy).is_none());
        }
        assert_eq!(
            rejection(&record("標題", &["基金會", "採購公告"], None), &policy),
            Some("administrative_category")
        );
        assert_eq!(
            rejection(&record("【徵才】研究員", &[], None), &policy),
            Some("administrative_title")
        );
        assert_eq!(
            rejection(&record("索馬利蘭專案志工招募", &[], None), &policy),
            Some("administrative_title")
        );
    }
    #[test]
    fn requested_source_label_and_title_mentions_are_not_publisher_evidence() {
        let policy = ItemFilters {
            publisher_allowlist: vec!["子館".into()],
            ..Default::default()
        };
        assert_eq!(
            rejection(&record("子館展覽新聞", &[], None), &policy),
            Some("publisher_not_proven")
        );
        assert_eq!(
            rejection(&record("子館展覽新聞", &[], Some("母部會")), &policy),
            Some("different_publisher")
        );
        assert!(rejection(&record("展覽新聞", &[], Some("子館")), &policy).is_none());
    }
}
