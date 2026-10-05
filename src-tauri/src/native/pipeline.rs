use crate::{policy::Profile, ranking::RankedBatch, scraper::NewsItem, ContentMode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const MIN_SUMMARY_CHARS: usize = 80;
const PREFILTER_VERSION: &str = "recall-shadow-v1";

pub(super) fn key(item: &NewsItem) -> String {
    // Use exactly the existing quality normalization, including tracking removal.
    let normalized = crate::scraper::quality::process(vec![item.clone()]);
    let item = normalized.items.first().unwrap_or(item);
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        item.source, item.date, item.title, item.link
    )
}

pub(super) fn decision(profile: &Profile, item: &NewsItem) -> Value {
    let normalize = crate::policy::normalized_name;
    let text = normalize(&format!("{} {}", item.title, item.summary));
    let source = normalize(&item.source);
    for topic in profile.initiatives.iter().filter(|topic| topic.enabled) {
        if [&topic.lead_source, &topic.lead_agency]
            .into_iter()
            .any(|name| {
                !name.is_empty()
                    && (source.contains(&normalize(name)) || text.contains(&normalize(name)))
            })
        {
            return json!({"decision":"candidate", "reason":"priority_source", "evidence":topic.name});
        }
        for term in std::iter::once(&topic.name)
            .chain(&topic.exact_phrases)
            .chain(&topic.strong_keywords)
            .chain(&topic.context_keywords)
            .chain(
                topic
                    .weighted_keywords
                    .iter()
                    .filter(|keyword| keyword.enabled)
                    .map(|keyword| &keyword.text),
            )
            .chain(&profile.general_keywords)
        {
            if !term.trim().is_empty() && text.contains(&normalize(term)) {
                return json!({"decision":"candidate", "reason":"policy_term", "evidence":term});
            }
        }
    }
    if item.summary.trim().chars().count() < MIN_SUMMARY_CHARS {
        json!({"decision":"uncertain", "reason":"insufficient_summary"})
    } else {
        // This is a shadow prediction only; no article is removed.
        json!({"decision":"rejected", "reason":"sufficient_summary_without_policy_evidence"})
    }
}

pub(super) fn rank(profile: &Profile, items: &[NewsItem], mode: ContentMode) -> RankedBatch {
    if mode == ContentMode::Full {
        crate::ranking::rank(profile, items)
    } else {
        RankedBatch {
            results: vec![
                json!({"evaluated":false, "relevance":null, "score":null,
                "bm25_score":null, "hard_excluded":false});
                items.len()
            ],
            excluded_count: 0,
            rule_counts: BTreeMap::new(),
            topic_counts: BTreeMap::new(),
        }
    }
}

pub(super) fn shadow(
    profile: &Profile,
    discoveries: &[NewsItem],
    final_items: &[NewsItem],
    classifications: &[Value],
    dedupe_affiliated: bool,
) -> Value {
    let early = if dedupe_affiliated {
        crate::scraper::quality::dedupe_affiliated(discoveries.to_vec())
    } else {
        discoveries.to_vec()
    };
    let quality = crate::scraper::quality::process(early);
    let predictions: BTreeMap<_, _> = quality
        .items
        .iter()
        .map(|item| (key(item), decision(profile, item)))
        .collect();
    let rejected: BTreeSet<_> = predictions
        .iter()
        .filter(|(_, value)| value["decision"] == "rejected")
        .map(|(key, _)| key.clone())
        .collect();
    let mut false_negatives = Vec::new();
    let mut dedup_differences = Vec::new();
    for (item, classification) in final_items.iter().zip(classifications) {
        let id = key(item);
        if !predictions.contains_key(&id) {
            dedup_differences.push(json!({"source":item.source, "date":item.date, "title":item.title,
                "link":item.link, "classification":classification, "reason":"early_quality_or_affiliated_dedup_changed_retained_item"}));
        }
        if rejected.contains(&id)
            && classification["hard_excluded"] != true
            && matches!(
                classification["relevance"].as_str(),
                Some("高度相關" | "可能相關")
            )
        {
            false_negatives.push(
                json!({"source":item.source, "date":item.date, "title":item.title,
                "link":item.link, "prefilter":predictions[&id], "classification":classification}),
            );
        }
    }
    false_negatives.sort_by_key(|value| value.to_string());
    dedup_differences.sort_by_key(|value| value.to_string());
    let mut seen = BTreeSet::new();
    let simulated_avoided = discoveries
        .iter()
        .filter(|item| {
            let id = key(item);
            let retained = seen.insert(id.clone());
            item.full_text.is_empty()
                && !item.link.is_empty()
                && (!retained || !predictions.contains_key(&id) || rejected.contains(&id))
        })
        .count();
    let hash = format!(
        "{:x}",
        Sha256::digest(format!(
            "{PREFILTER_VERSION}:{MIN_SUMMARY_CHARS}:{}",
            profile.hash()
        ))
    );
    json!({"mode":"shadow", "version":PREFILTER_VERSION, "ruleset_hash":hash,
        "prefilter_input_count":quality.items.len(), "prefilter_candidate_count":predictions.len()-rejected.len(),
        "prefilter_uncertain_count":predictions.values().filter(|value| value["decision"] == "uncertain").count(),
        "prefilter_rejected_count":rejected.len(), "early_duplicate_count":quality.duplicate_count,
        "detail_requests_avoided":0, "simulated_detail_requests_avoided":simulated_avoided,
        "false_negative_high_count":false_negatives.iter().filter(|value| value["classification"]["relevance"] == "高度相關").count(),
        "false_negative_possible_count":false_negatives.iter().filter(|value| value["classification"]["relevance"] == "可能相關").count(),
        "early_dedup_high_relevance_loss_count":dedup_differences.iter().filter(|value| value["classification"]["relevance"] == "高度相關" && value["classification"]["hard_excluded"] != true).count(),
        "false_negatives":false_negatives, "early_dedup_differences":dedup_differences,
        "predictions":quality.items.iter().map(|item| json!({"source":item.source, "date":item.date,
            "title":item.title, "link":item.link, "list_summary":item.summary, "prefilter":predictions[&key(item)]})).collect::<Vec<_>>()})
}

/// Compare the modes using the same discovery snapshot, without extra requests.
pub(super) fn summary_comparison(
    discoveries: &[NewsItem],
    items: &[NewsItem],
    classifications: &[Value],
    affiliated: bool,
) -> Value {
    let summary_items = if affiliated {
        crate::scraper::quality::dedupe_affiliated(discoveries.to_vec())
    } else {
        discoveries.to_vec()
    };
    let summary = crate::scraper::quality::process(summary_items);
    let summary_keys: BTreeSet<_> = summary.items.iter().map(key).collect();
    let full_keys: BTreeSet<_> = items
        .iter()
        .zip(classifications)
        .filter(|(_, result)| result["hard_excluded"] != true)
        .map(|(item, _)| key(item))
        .collect();
    let excluded: BTreeSet<_> = items
        .iter()
        .zip(classifications)
        .filter(|(_, result)| result["hard_excluded"] == true)
        .map(|(item, _)| key(item))
        .collect();
    let mut differences = Vec::new();
    for item in &summary.items {
        let id = key(item);
        if !full_keys.contains(&id) {
            differences.push(json!({"source":item.source, "date":item.date, "title":item.title, "link":item.link,
                "present_in":"summary", "reason":if excluded.contains(&id) {"full_topic_hard_exclusion"} else {"quality_or_affiliated_dedup_changed_retained_item"}}));
        }
    }
    for item in items {
        let id = key(item);
        if full_keys.contains(&id) && !summary_keys.contains(&id) {
            differences.push(
                json!({"source":item.source, "date":item.date, "title":item.title, "link":item.link,
                "present_in":"full", "reason":"quality_or_affiliated_dedup_changed_retained_item"}),
            );
        }
    }
    differences.sort_by_key(|value| value.to_string());
    json!({"status":"same_discovery_simulation", "full_count":full_keys.len(), "summary_count":summary_keys.len(), "differences":differences})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> NewsItem {
        NewsItem {
            source: "測試機關".into(),
            date: "2026-09-21".into(),
            department: String::new(),
            title: "花園活動".into(),
            link: "https://example.test/news".into(),
            category: String::new(),
            summary: String::new(),
            full_text: String::new(),
            date_source: "published".into(),
        }
    }

    #[test]
    fn missing_summary_is_uncertain_and_policy_hit_is_candidate() {
        let profile = Profile::embedded();
        let mut article = item();
        assert_eq!(decision(&profile, &article)["decision"], "uncertain");
        article.title = profile
            .initiatives
            .iter()
            .find(|topic| topic.enabled)
            .unwrap()
            .name
            .clone();
        assert_eq!(decision(&profile, &article)["decision"], "candidate");
    }

    #[test]
    fn shadow_records_false_negative_without_changing_articles() {
        let profile = Profile::embedded();
        let mut article = item();
        article.summary = "花園種植花草景觀活動".repeat(20);
        let input = vec![article.clone(), article.clone()];
        let report = shadow(
            &profile,
            &input,
            &[article.clone()],
            &[json!({"relevance":"高度相關"})],
            false,
        );
        assert_eq!(report["false_negative_high_count"], 1);
        assert_eq!(report["early_duplicate_count"], 1);
        assert_eq!(report["detail_requests_avoided"], 0);
        assert_eq!(report["simulated_detail_requests_avoided"], 2);
        assert_eq!(input, vec![article.clone(), article]);
    }

    #[test]
    fn summary_never_classifies_or_hard_excludes() {
        let result = rank(&Profile::embedded(), &[item()], ContentMode::Summary);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0]["evaluated"], false);
        assert!(result.results[0]["score"].is_null());
        assert!(result.results[0]["bm25_score"].is_null());
        assert_eq!(result.excluded_count, 0);
    }

    #[test]
    fn mode_comparison_records_topic_exclusions_per_article() {
        let article = item();
        let report = summary_comparison(
            std::slice::from_ref(&article),
            std::slice::from_ref(&article),
            &[json!({"hard_excluded":true})],
            false,
        );
        assert_eq!(report["summary_count"], 1);
        assert_eq!(report["full_count"], 0);
        assert_eq!(
            report["differences"][0]["reason"],
            "full_topic_hard_exclusion"
        );
        assert_eq!(report["differences"][0]["link"], article.link);
    }
}
