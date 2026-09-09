//! One deterministic BM25 corpus per run. No network or persistent index required.
use crate::{
    policy::{Initiative, Profile},
    relevance::{classify_with_profile, normalize, rule_matches},
    scraper::NewsItem,
};
use jieba_rs::Jieba;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

type Terms = BTreeMap<String, usize>;
pub struct RankedBatch {
    pub results: Vec<Value>,
    pub excluded_count: usize,
    pub rule_counts: BTreeMap<String, usize>,
    pub topic_counts: BTreeMap<String, usize>,
}
fn terms(jieba: &Jieba, text: &str) -> Terms {
    let text = normalize(text);
    let mut out = Terms::new();
    for token in jieba.cut_for_search(&text, false) {
        let word = token.word;
        if word.chars().count() < 2
            || !word.chars().any(char::is_alphanumeric)
            || [
                "以及", "相關", "推動", "透過", "辦理", "進行", "提供", "the", "and",
            ]
            .contains(&word)
        {
            continue;
        }
        *out.entry(word.to_owned()).or_default() += 1;
    }
    out
}
fn bm25(
    doc: &Terms,
    query: &BTreeMap<String, f64>,
    df: &Terms,
    n: usize,
    avg: f64,
    p: &Profile,
) -> f64 {
    let len = doc.values().sum::<usize>() as f64;
    query
        .iter()
        .map(|(term, weight)| {
            let tf = *doc.get(term).unwrap_or(&0) as f64;
            if tf == 0.0 {
                return 0.0;
            }
            let freq = *df.get(term).unwrap_or(&0) as f64;
            let idf = (1.0 + (n as f64 - freq + 0.5) / (freq + 0.5)).ln();
            let s = &p.scoring;
            weight * idf * tf * (s.k1 + 1.0) / (tf + s.k1 * (1.0 - s.b + s.b * len / avg.max(1.0)))
        })
        .sum()
}
fn keyword_pairs(t: &Initiative) -> Vec<(String, f64)> {
    t.strong_keywords
        .iter()
        .map(|s| (s.clone(), 3.0))
        .chain(t.context_keywords.iter().map(|s| (s.clone(), 1.0)))
        .chain(
            t.weighted_keywords
                .iter()
                .filter(|k| k.enabled)
                .map(|k| (k.text.clone(), k.weight)),
        )
        .chain(t.exact_phrases.iter().map(|s| (s.clone(), 3.0)))
        .chain(std::iter::once((t.name.clone(), 3.0)))
        .collect()
}
pub fn rank(profile: &Profile, items: &[NewsItem]) -> RankedBatch {
    let mut recall_jieba = Jieba::new();
    for word in include_str!("../resources/policy-dictionary.txt")
        .lines()
        .filter(|w| !w.trim().is_empty())
    {
        recall_jieba.add_word(word.trim(), Some(10000), None);
    }
    // Recall uses domain words without adding entire query phrases, so separated words stay comparable.
    let mut jieba = recall_jieba.clone();
    for topic in profile.initiatives.iter().filter(|t| t.enabled) {
        for (word, _) in keyword_pairs(topic) {
            jieba.add_word(&normalize(&word), None, None);
        }
    }
    let titles: Vec<Terms> = items.iter().map(|n| terms(&jieba, &n.title)).collect();
    let summaries: Vec<Terms> = items.iter().map(|n| terms(&jieba, &n.summary)).collect();
    let mut df_title = Terms::new();
    let mut df_summary = Terms::new();
    for doc in &titles {
        for word in doc.keys() {
            *df_title.entry(word.clone()).or_default() += 1;
        }
    }
    for doc in &summaries {
        for word in doc.keys() {
            *df_summary.entry(word.clone()).or_default() += 1;
        }
    }
    let n = items.len();
    let avg_title = titles.iter().flat_map(|d| d.values()).sum::<usize>() as f64 / n.max(1) as f64;
    let avg_summary =
        summaries.iter().flat_map(|d| d.values()).sum::<usize>() as f64 / n.max(1) as f64;
    let mut batch = RankedBatch {
        results: vec![],
        excluded_count: 0,
        rule_counts: BTreeMap::new(),
        topic_counts: BTreeMap::new(),
    };
    let queries: Vec<_> = profile
        .initiatives
        .iter()
        .filter(|t| t.enabled)
        .map(|topic| {
            let mut query = BTreeMap::<String, f64>::new();
            for (word, weight) in keyword_pairs(topic).into_iter().chain(
                profile
                    .general_keywords
                    .iter()
                    .map(|s| (s.clone(), profile.scoring.general_weight)),
            ) {
                for token in terms(&jieba, &word).keys() {
                    query
                        .entry(token.clone())
                        .and_modify(|w| *w = w.max(weight))
                        .or_insert(weight);
                }
            }
            batch.topic_counts.insert(topic.name.clone(), 0);
            let mut one = profile.clone();
            one.initiatives = vec![topic.clone()];
            one.initiatives[0].penalty_keywords.clear();
            (topic, query, one)
        })
        .collect();
    for (i, item) in items.iter().enumerate() {
        let mut topic_matches = vec![];
        let mut any_hard = false;
        let recall_title = terms(&recall_jieba, &item.title);
        let recall_summary = terms(&recall_jieba, &item.summary);
        for (topic, query, one) in &queries {
            let mut result = classify_with_profile(one, &item.title, &item.source, &item.summary);
            let mut score = result["score"].as_u64().unwrap_or(0) as u32;
            let mut matched = result["matched_keywords"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut reasons = result["reasons"].as_array().cloned().unwrap_or_default();
            // General AI words alone do not assign every enabled topic.
            let assigned = result["topics"]
                .as_array()
                .is_some_and(|ts| ts.iter().any(|t| t.as_str() == Some(&topic.name)));
            if !assigned {
                score = 0;
                matched.clear();
                reasons.clear();
            }
            if score < profile.thresholds.possible {
                for word in topic.strong_keywords.iter().chain(
                    topic
                        .weighted_keywords
                        .iter()
                        .filter(|k| k.enabled && k.weight >= 3.0)
                        .map(|k| &k.text),
                ) {
                    let tokens: BTreeSet<_> = terms(&recall_jieba, word)
                        .keys()
                        .filter(|&w| !profile.general_keywords.iter().any(|g| normalize(g) == *w))
                        .cloned()
                        .collect();
                    if tokens.len() >= 2
                        && [&recall_title, &recall_summary]
                            .iter()
                            .any(|doc| tokens.iter().all(|w| doc.contains_key(w)))
                    {
                        score = profile.thresholds.possible;
                        matched.push(json!(word));
                        reasons.push(json!("核心片語斷詞全部命中，列入可能相關"));
                    }
                }
            }
            if score == 0 {
                continue;
            }
            let hard: Vec<_> = topic
                .exclude_keywords
                .iter()
                .filter(|r| rule_matches(r, &item.title, &item.summary))
                .collect();
            let hard_excluded = !hard.is_empty();
            let penalties: Vec<_> = topic
                .penalty_keywords
                .iter()
                .filter(|r| rule_matches(r, &item.title, &item.summary))
                .collect();
            let penalty = penalties.iter().map(|r| r.penalty).max().unwrap_or(0);
            let excluded: Vec<_> = if hard_excluded {
                hard.iter().map(|r| r.text.clone()).collect()
            } else {
                penalties.iter().map(|r| r.text.clone()).collect()
            };
            if hard_excluded {
                any_hard = true;
                score = 0;
                reasons.push(json!("命中本主題完全排除詞"));
            } else if penalty > 0 {
                score = score.saturating_sub(penalty);
                reasons.push(json!(format!("本主題扣分 {penalty} 分（取最高值）")));
            }
            for word in &excluded {
                *batch
                    .rule_counts
                    .entry(format!(
                        "{}／{}／{}",
                        topic.name,
                        if hard_excluded {
                            "完全排除"
                        } else {
                            "扣分"
                        },
                        word
                    ))
                    .or_default() += 1;
            }
            let relevance = if hard_excluded {
                "完全排除"
            } else if score >= profile.thresholds.high {
                "高度相關"
            } else if score >= profile.thresholds.possible {
                "可能相關"
            } else {
                "未納入"
            };
            let raw = profile.scoring.title_weight
                * bm25(&titles[i], query, &df_title, n, avg_title, profile)
                + profile.scoring.summary_weight
                    * bm25(&summaries[i], query, &df_summary, n, avg_summary, profile);
            if score >= profile.thresholds.possible {
                *batch.topic_counts.get_mut(&topic.name).unwrap() += 1;
            }
            result = json!({"name":topic.name,"priority_sources":[topic.lead_source],"score":score,"bm25_score":raw,"relevance":relevance,"matched_keywords":matched,"excluded_keywords":excluded,"reasons":reasons,"hard_excluded":hard_excluded});
            topic_matches.push(result);
        }
        let accepted: Vec<_> = topic_matches
            .iter()
            .filter(|v| v["score"].as_u64().unwrap_or(0) >= profile.thresholds.possible as u64)
            .collect();
        let hard_excluded = any_hard && accepted.is_empty();
        if hard_excluded {
            batch.excluded_count += 1;
        }
        let display_matches: Vec<_> = if accepted.is_empty() {
            topic_matches
                .iter()
                .filter(|v| v["hard_excluded"] != true)
                .collect()
        } else {
            accepted.clone()
        };
        let score = display_matches
            .iter()
            .filter_map(|v| v["score"].as_u64())
            .max()
            .unwrap_or(0) as u32;
        let bm25 = display_matches
            .iter()
            .filter(|v| {
                v["relevance"]
                    == if score >= profile.thresholds.high {
                        "高度相關"
                    } else if score >= profile.thresholds.possible {
                        "可能相關"
                    } else {
                        "未納入"
                    }
            })
            .filter_map(|v| v["bm25_score"].as_f64())
            .fold(0.0, f64::max);
        let mut result = if topic_matches.is_empty() {
            classify_with_profile(profile, &item.title, &item.source, &item.summary)
        } else {
            let flatten = |key: &str| {
                display_matches
                    .iter()
                    .flat_map(|v| v[key].as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect::<BTreeSet<_>>()
            };
            json!({"score":score,"relevance":if score>=profile.thresholds.high {"高度相關"} else if score>=profile.thresholds.possible {"可能相關"} else {"未納入"},"topics":accepted.iter().map(|t|t["name"].clone()).collect::<Vec<_>>(),"priority_sources":flatten("priority_sources"),"matched_keywords":flatten("matched_keywords"),"excluded_keywords":topic_matches.iter().flat_map(|t|t["excluded_keywords"].as_array().into_iter().flatten()).cloned().collect::<Vec<_>>(),"reasons":flatten("reasons")})
        };
        result["topic_matches"] = json!(topic_matches);
        result["bm25_score"] = json!(bm25);
        result["hard_excluded"] = json!(hard_excluded);
        batch.results.push(result);
    }
    batch
}
