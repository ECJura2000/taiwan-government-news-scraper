use regex::Regex;
use serde_json::Value;

use crate::policy::Profile as Ruleset;
fn ruleset() -> Ruleset {
    Ruleset::embedded()
}

pub fn default_summary() -> serde_json::Value {
    ruleset().summary()
}

pub fn policy_document() -> Value {
    serde_json::from_str(include_str!("../resources/relevance-policy.json"))
        .expect("embedded relevance policy must be valid JSON")
}

pub fn classify(title: &str, source: &str, summary: &str) -> serde_json::Value {
    classify_with_profile(&ruleset(), title, source, summary)
}

pub fn classify_with_profile(
    profile: &Ruleset,
    title: &str,
    source: &str,
    summary: &str,
) -> serde_json::Value {
    let normalized_title = normalize(title);
    let normalized_summary = normalize(summary);
    let title_global = find_matches(&normalized_title, &profile.general_keywords);
    let summary_global = find_matches(&normalized_summary, &profile.general_keywords);
    let exclusions: Vec<String> = Vec::new();
    let mut candidates = Vec::new();
    let mut matches = Vec::new();

    for initiative in profile.initiatives.iter().filter(|t| t.enabled) {
        let title_name = if contains(&normalized_title, &initiative.name)
            || initiative
                .exact_phrases
                .iter()
                .any(|p| contains(&normalized_title, p))
        {
            vec![initiative.name.clone()]
        } else {
            Vec::new()
        };
        let summary_name = if contains(&normalized_summary, &initiative.name)
            || initiative
                .exact_phrases
                .iter()
                .any(|p| contains(&normalized_summary, p))
        {
            vec![initiative.name.clone()]
        } else {
            Vec::new()
        };
        let mut core = initiative.strong_keywords.clone();
        core.extend(
            initiative
                .weighted_keywords
                .iter()
                .filter(|k| k.enabled && k.weight >= 3.0)
                .map(|k| k.text.clone()),
        );
        let mut supporting = initiative.context_keywords.clone();
        supporting.extend(
            initiative
                .weighted_keywords
                .iter()
                .filter(|k| k.enabled && k.weight < 3.0)
                .map(|k| k.text.clone()),
        );
        let exclusions = initiative
            .penalty_keywords
            .iter()
            .filter(|r| rule_matches(r, title, summary))
            .map(|r| r.text.clone())
            .collect::<Vec<_>>();
        let penalty = initiative
            .penalty_keywords
            .iter()
            .filter(|r| rule_matches(r, title, summary))
            .map(|r| r.penalty)
            .max()
            .unwrap_or(0);
        let title_core = find_matches(&normalized_title, &core);
        let summary_core = find_matches(&normalized_summary, &core);
        let title_supporting = find_matches(&normalized_title, &supporting);
        let summary_supporting = find_matches(&normalized_summary, &supporting);
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
            score = score.saturating_sub(penalty);
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
        if score >= u64::from(profile.thresholds.high) {
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

pub(crate) fn normalize(value: &str) -> String {
    crate::policy::normalized_name(value)
}

pub(crate) fn contains(text: &str, keyword: &str) -> bool {
    let keyword = normalize(keyword);
    if keyword.is_empty() {
        return false;
    }
    if keyword.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Regex::new(&format!(
            r"(?:^|[^a-z0-9]){}(?:$|[^a-z0-9])",
            regex::escape(&keyword)
        ))
        .is_ok_and(|p| p.is_match(text));
    }
    text.replace(' ', "").contains(&keyword.replace(' ', ""))
}
pub(crate) fn rule_matches(rule: &crate::policy::NegativeRule, title: &str, summary: &str) -> bool {
    rule.enabled
        && rule.match_fields.iter().any(|field| {
            contains(
                &normalize(if field == "title" { title } else { summary }),
                &rule.text,
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_policy_has_ten_topics_and_stable_hash() {
        let summary = default_summary();
        assert_eq!(summary["topic_count"], 10);
        assert!(summary["keyword_count"].as_u64().unwrap() > 107);
        assert_eq!(summary["schema_version"], 2);
        assert_eq!(summary["ruleset_hash"].as_str().unwrap().len(), 64);
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
            let legacy =
                Ruleset::parse(include_str!("../tests/fixtures/legacy-policy.json")).unwrap();
            let result = classify_with_profile(&legacy, columns[1], columns[0], columns[2]);
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
