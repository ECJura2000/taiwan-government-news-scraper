use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
struct Ruleset {
    version: String,
    initiatives: Vec<Initiative>,
    general_keywords: Vec<String>,
    negative_keywords: Vec<String>,
    thresholds: Thresholds,
}

#[derive(Debug, Clone, Deserialize)]
struct Initiative {
    name: String,
    lead_source: String,
    strong_keywords: Vec<String>,
    context_keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Thresholds {
    high: u32,
    possible: u32,
    negative_penalty: u32,
}

fn ruleset() -> Ruleset {
    serde_json::from_str(include_str!("../resources/relevance-policy.json"))
        .expect("embedded relevance policy must be valid JSON")
}

pub fn default_summary() -> serde_json::Value {
    let profile = ruleset();
    let hash = effective_profile_hash(&profile);
    let keyword_count = profile.general_keywords.len()
        + profile
            .initiatives
            .iter()
            .map(|initiative| initiative.strong_keywords.len() + initiative.context_keywords.len())
            .sum::<usize>();
    serde_json::json!({
        "name": "AI 新十大建設預設範本",
        "schema_version": 1,
        "template_version": profile.version,
        "ruleset_hash": hash,
        "source": "內建預設範本",
        "topic_count": profile.initiatives.len(),
        "enabled_topic_count": profile.initiatives.len(),
        "disabled_topic_count": 0,
        "keyword_count": keyword_count,
        "enabled_keyword_count": keyword_count,
        "disabled_keyword_count": 0,
        "exclusion_count": profile.negative_keywords.len(),
        "enabled_exclusion_count": profile.negative_keywords.len(),
        "disabled_exclusion_count": 0,
        "custom_item_count": 0,
        "deleted_default_count": 0,
    })
}

pub fn policy_document() -> Value {
    serde_json::from_str(include_str!("../resources/relevance-policy.json"))
        .expect("embedded relevance policy must be valid JSON")
}

pub fn classify(title: &str, source: &str, summary: &str) -> serde_json::Value {
    let profile = ruleset();
    let normalized_title = normalize(title);
    let normalized_summary = normalize(summary);
    let title_global = find_matches(&normalized_title, &profile.general_keywords);
    let summary_global = find_matches(&normalized_summary, &profile.general_keywords);
    let exclusions = find_matches(&normalized_title, &profile.negative_keywords);
    let mut candidates = Vec::new();
    let mut matches = Vec::new();

    for initiative in &profile.initiatives {
        let title_name = if contains(&normalized_title, &initiative.name) {
            vec![initiative.name.clone()]
        } else {
            Vec::new()
        };
        let summary_name = if contains(&normalized_summary, &initiative.name) {
            vec![initiative.name.clone()]
        } else {
            Vec::new()
        };
        let title_core = find_matches(&normalized_title, &initiative.strong_keywords);
        let summary_core = find_matches(&normalized_summary, &initiative.strong_keywords);
        let title_supporting = find_matches(&normalized_title, &initiative.context_keywords);
        let summary_supporting = find_matches(&normalized_summary, &initiative.context_keywords);
        let priority_source = source == initiative.lead_source;
        let mut scores: Vec<u32> = Vec::new();
        let mut reasons = Vec::new();
        if !title_name.is_empty() {
            scores.push(100);
            reasons.push("標題命中完整主題名稱");
        }
        if !title_core.is_empty() {
            scores.push(85);
            reasons.push("標題命中核心詞");
        }
        if !title_supporting.is_empty() && !title_global.is_empty() {
            scores.push(if priority_source { 80 } else { 65 });
            reasons.push("標題同時命中脈絡詞與輔助詞");
        } else if !title_supporting.is_empty() && priority_source {
            scores.push(50);
            reasons.push("優先關聯機關標題命中輔助詞");
        }
        if !summary_name.is_empty() {
            scores.push(70);
            reasons.push("摘要命中完整主題名稱");
        }
        if !summary_core.is_empty() {
            scores.push(60);
            reasons.push("摘要命中核心詞");
        }
        if !summary_supporting.is_empty()
            && (!summary_global.is_empty() || !title_global.is_empty())
        {
            scores.push(if priority_source { 55 } else { 45 });
            reasons.push("摘要同時命中脈絡詞與輔助詞");
        } else if !summary_supporting.is_empty() && priority_source {
            scores.push(40);
            reasons.push("優先關聯機關摘要命中輔助詞");
        }
        let Some(mut score) = scores.into_iter().max() else {
            continue;
        };
        if !exclusions.is_empty() {
            score = score.saturating_sub(profile.thresholds.negative_penalty);
            reasons.push("命中排除詞，分數下修");
        }
        let relevance = if score >= profile.thresholds.high {
            "高度相關"
        } else if score >= profile.thresholds.possible {
            "可能相關"
        } else {
            "未納入"
        };
        let keywords = unique(
            [
                title_name,
                title_core,
                title_supporting,
                title_global.clone(),
                summary_name,
                summary_core,
                summary_supporting,
                summary_global.clone(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        );
        let candidate = serde_json::json!({
            "name": initiative.name,
            "priority_sources": [initiative.lead_source],
            "relevance": relevance,
            "score": score,
            "matched_keywords": keywords,
            "excluded_keywords": exclusions,
            "reasons": unique(reasons.into_iter().map(String::from).collect()),
        });
        candidates.push(candidate.clone());
        if score >= profile.thresholds.possible {
            matches.push(candidate);
        }
    }

    let all_context = unique(
        [title_global, summary_global]
            .into_iter()
            .flatten()
            .collect(),
    );
    if matches.is_empty() && !candidates.is_empty() {
        let score = candidates
            .iter()
            .filter_map(|item| item["score"].as_u64())
            .max()
            .unwrap_or(0);
        return aggregate_candidates(candidates, "未納入", score as u32);
    }
    if matches.is_empty() && !all_context.is_empty() {
        let score = if exclusions.is_empty() {
            profile.thresholds.possible
        } else {
            profile
                .thresholds
                .possible
                .saturating_sub(profile.thresholds.negative_penalty)
        };
        let relevance = if score >= profile.thresholds.possible {
            "可能相關"
        } else {
            "未納入"
        };
        let reasons: Vec<String> = if exclusions.is_empty() {
            vec!["僅命中全域脈絡詞，需人工判讀".into()]
        } else {
            vec![
                "僅命中全域脈絡詞，需人工判讀".into(),
                "命中排除詞，分數下修".into(),
            ]
        };
        let topic_match = serde_json::json!({
            "id": "",
            "name": "待人工判讀",
            "priority_sources": [],
            "relevance": relevance,
            "score": score,
            "matched_keywords": all_context,
            "excluded_keywords": exclusions,
            "reasons": reasons,
        });
        return result_json(
            relevance,
            score,
            vec!["待人工判讀".into()],
            topic_match["matched_keywords"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            topic_match["excluded_keywords"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            topic_match["reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            vec![topic_match],
        );
    }
    if matches.is_empty() {
        let relevance = if exclusions.is_empty() {
            ""
        } else {
            "未納入"
        };
        return result_json(
            relevance,
            0,
            Vec::new(),
            Vec::new(),
            exclusions,
            Vec::new(),
            candidates,
        );
    }
    let score = matches
        .iter()
        .filter_map(|item| item["score"].as_u64())
        .max()
        .unwrap_or(0);
    aggregate_candidates(
        candidates,
        if score >= 80 {
            "高度相關"
        } else {
            "可能相關"
        },
        score as u32,
    )
}

fn aggregate_candidates(candidates: Vec<Value>, relevance: &str, score: u32) -> Value {
    let topics = unique(
        candidates
            .iter()
            .filter_map(|item| item["name"].as_str().map(String::from))
            .collect(),
    );
    let keywords = unique(
        candidates
            .iter()
            .flat_map(|item| {
                item["matched_keywords"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(String::from)
            })
            .collect(),
    );
    let exclusions = unique(
        candidates
            .iter()
            .flat_map(|item| {
                item["excluded_keywords"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(String::from)
            })
            .collect(),
    );
    let reasons = unique(
        candidates
            .iter()
            .flat_map(|item| {
                let name = item["name"].as_str().unwrap_or_default();
                item["reasons"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(move |reason| format!("{name}：{reason}"))
            })
            .collect(),
    );
    result_json(
        relevance, score, topics, keywords, exclusions, reasons, candidates,
    )
}

fn result_json(
    relevance: &str,
    score: u32,
    topics: Vec<String>,
    keywords: Vec<String>,
    exclusions: Vec<String>,
    reasons: Vec<String>,
    topic_matches: Vec<Value>,
) -> Value {
    let priority_sources = unique(
        topic_matches
            .iter()
            .flat_map(|item| {
                item["priority_sources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(String::from)
            })
            .collect(),
    );
    serde_json::json!({
        "relevance": relevance,
        "score": score,
        "topics": topics,
        "priority_sources": priority_sources,
        "matched_keywords": keywords,
        "excluded_keywords": exclusions,
        "reasons": reasons,
        "topic_matches": topic_matches,
    })
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(|character| character.to_lowercase())
        .collect::<String>()
        .replace('（', "(")
        .replace('）', ")")
}

fn contains(text: &str, keyword: &str) -> bool {
    let keyword = normalize(keyword);
    if keyword.is_empty() {
        return false;
    }
    if keyword.len() <= 4
        && keyword
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Regex::new(&format!(
            r"(?i)(?:^|[^a-z0-9]){}(?:$|[^a-z0-9])",
            regex::escape(&keyword)
        ))
        .is_ok_and(|pattern| pattern.is_match(text));
    }
    text.contains(&keyword)
}

fn find_matches(text: &str, keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .filter(|keyword| contains(text, keyword))
        .cloned()
        .collect()
}

fn unique(values: Vec<String>) -> Vec<String> {
    values.into_iter().fold(Vec::new(), |mut result, value| {
        if !result.contains(&value) {
            result.push(value);
        }
        result
    })
}

pub(crate) fn stable_default_id(kind: &str, parts: &[&str]) -> String {
    let payload = std::iter::once(kind.to_owned())
        .chain(parts.iter().map(|part| normalize(part)))
        .collect::<Vec<_>>()
        .join("|");
    let digest = Sha256::digest(payload.as_bytes());
    format!(
        "default:{kind}:{}",
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn keyword_rule(kind: &str, topic: Option<&str>, keyword: &str) -> Value {
    let id = match topic {
        Some(topic) => stable_default_id(kind, &[topic, keyword]),
        None => stable_default_id(kind, &[keyword]),
    };
    serde_json::json!({
        "id": id,
        "text": keyword,
        "enabled": true,
        "origin": "default",
    })
}

fn effective_profile_payload(profile: &Ruleset) -> Value {
    let topics = profile
        .initiatives
        .iter()
        .map(|initiative| {
            serde_json::json!({
                "id": stable_default_id("topic", &[&initiative.name]),
                "name": initiative.name,
                "enabled": true,
                "match_name": true,
                "priority_sources": [initiative.lead_source],
                "core_keywords": initiative.strong_keywords.iter()
                    .map(|keyword| keyword_rule("core", Some(&initiative.name), keyword))
                    .collect::<Vec<_>>(),
                "supporting_keywords": initiative.context_keywords.iter()
                    .map(|keyword| keyword_rule("supporting", Some(&initiative.name), keyword))
                    .collect::<Vec<_>>(),
                "context_keywords": [],
            })
        })
        .collect::<Vec<_>>();
    let global_context_keywords = profile
        .general_keywords
        .iter()
        .map(|keyword| keyword_rule("global-context", None, keyword))
        .collect::<Vec<_>>();
    let exclusions = profile
        .negative_keywords
        .iter()
        .map(|keyword| {
            serde_json::json!({
                "id": stable_default_id("exclusion", &[keyword]),
                "text": keyword,
                "topic_id": "",
                "match_fields": ["title"],
                "enabled": true,
                "origin": "default",
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema_version": 1,
        "include_unassigned_context_matches": true,
        "topics": topics,
        "global_context_keywords": global_context_keywords,
        "exclusions": exclusions,
    })
}

fn effective_profile_hash(profile: &Ruleset) -> String {
    let digest = Sha256::digest(canonical_json(&effective_profile_payload(profile)).as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).expect("string serializes"),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values.iter().collect();
            format!(
                "{{{}}}",
                sorted
                    .into_iter()
                    .map(|(key, value)| {
                        format!(
                            "{}:{}",
                            serde_json::to_string(key).expect("key serializes"),
                            canonical_json(value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_policy_has_ten_topics_and_stable_hash() {
        let summary = default_summary();
        assert_eq!(summary["topic_count"], 10);
        assert_eq!(summary["keyword_count"], 107);
        assert_eq!(summary["template_version"], "2.1.2");
        assert_eq!(summary["ruleset_hash"], "e75be08ee1c5dab8");
    }

    #[test]
    fn relevance_classifier_preserves_high_score_and_exclusion_penalty() {
        let high = classify("主權AI及算力建設正式啟動", "國科會", "國家算力建設");
        assert_eq!(high["relevance"], "高度相關");
        assert_eq!(high["score"], 100);

        let excluded = classify("AI算力採購公告", "國科會", "");
        assert_eq!(excluded["score"], 35);
        assert_eq!(excluded["relevance"], "未納入");
    }

    #[test]
    fn labeled_python_fixture_keeps_relevance_and_topic_parity() {
        let fixture = include_str!("../tests/fixtures/relevance_labeled.tsv");
        for (index, line) in fixture.lines().skip(1).enumerate() {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert!(columns.len() >= 5, "fixture row {} is invalid", index + 2);
            let result = classify(columns[1], columns[0], columns[2]);
            let actual_relevance = result["relevance"].as_str().unwrap_or("");
            if columns[3].is_empty() {
                assert!(
                    actual_relevance.is_empty() || actual_relevance == "未納入",
                    "negative relevance mismatch at fixture row {}",
                    index + 2
                );
            } else {
                assert_eq!(
                    actual_relevance,
                    columns[3],
                    "relevance mismatch at fixture row {}",
                    index + 2
                );
            }
            let actual_topics = result["topics"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let expected_topics = columns[4]
                .split('|')
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            assert_eq!(
                actual_topics,
                expected_topics,
                "topic mismatch at fixture row {}",
                index + 2
            );
        }
    }
}
