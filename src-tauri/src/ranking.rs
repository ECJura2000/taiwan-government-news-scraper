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

// A long article may mention many initiatives in unrelated paragraphs. Only the
// opening evidence is allowed to establish membership; the full text still ranks it.
fn evidence_sentences(summary: &str) -> Vec<&str> {
    let end = summary
        .char_indices()
        .nth(900)
        .map(|(index, _)| index)
        .unwrap_or(summary.len());
    summary[..end]
        .split(['。', '！', '？', '!', '?', '\n', '\r'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .take(8)
        .collect()
}

fn has_government_context(sentence: &str) -> bool {
    [
        "政府",
        "機關",
        "行政",
        "公務",
        "跨機關",
        "公共服務",
        "通關",
        "稽查",
    ]
    .iter()
    .any(|word| sentence.contains(word))
}

fn topic_evidence_allowed(topic: &Initiative, sentence: &str) -> bool {
    let sentence = normalize(sentence);
    let has_any = |words: &[&str]| words.iter().any(|word| sentence.contains(word));
    match topic.name.as_str() {
        "智慧政府與資料治理" => {
            let explicit_application = has_any(&[
                "智慧稽查",
                "公務ai應用",
                "通關智慧特助",
                "貨物分類智慧助理",
                "業務助手",
                "服務型智慧政府",
            ]);
            let ai_application = has_government_context(&sentence)
                && has_any(&["ai", "人工智慧"])
                && has_any(&[
                    "ai應用",
                    "ai工具",
                    "ai導入",
                    "智慧服務",
                    "決策輔助",
                    "行政助手",
                    "資料匯流",
                    "共用模組",
                ]);
            let public_data_action = has_any(&["跨機關資料", "政府資料", "資料匯流"])
                && has_any(&["流通", "介接", "串接", "整合", "開放", "建置"]);
            let launched_data_program =
                sentence.contains("智慧政府2.0") && has_any(&["啟動", "推出", "資料治理"]);
            explicit_application || ai_application || public_data_action || launched_data_program
        }
        "全民智慧生活圈" => {
            let medical_application = has_any(&[
                "醫療服務",
                "健康照護",
                "疾病早篩",
                "遠距診療",
                "ai醫療",
                "ai藥物研發",
                "智慧醫療",
                "數位醫療",
                "遠距醫療",
                "健康資料治理",
                "醫療資料互通",
            ]);
            let healthcare_technology = has_any(&[
                "ai",
                "人工智慧",
                "醫療資料",
                "健康資料",
                "數位醫療",
                "遠距醫療",
            ]);
            let service_action = has_any(&[
                "服務", "導入", "應用", "建置", "開發", "診斷", "治療", "照護", "早篩", "互通",
                "臨床",
            ]);
            healthcare_technology && medical_application && service_action
        }
        "主權AI及算力建設" => {
            let concrete_asset = has_any(&[
                "主權ai訓練語料",
                "訓練語料庫",
                "國家算力",
                "算力中心",
                "ai模型訓練",
                "模型訓練",
            ]);
            let implementation = has_any(&[
                "建置", "建構", "上線", "開發", "部署", "擴增", "推出", "開放",
            ]);
            sentence.contains("主權ai") && concrete_asset && implementation
        }
        "千億資金驅動創新" => {
            let fund_or_program = has_any(&[
                "國發基金",
                "ai新創",
                "新創ai",
                "ai創業",
                "創業團隊",
                "ai新十大建設",
                "加強投資ai新創實施方案",
                "百億投資方案",
                "百億投資平台",
                "千億資金",
            ]);
            let investment_action = has_any(&[
                "國發基金投資",
                "新創投資",
                "投資平台",
                "資金媒合",
                "創業投資",
                "投資方案",
                "融資方案",
                "募資",
                "天使投資",
            ]);
            fund_or_program && investment_action
        }
        _ => true,
    }
}

fn strongest_evidence(profile: &Profile, title: &str, summary: &str) -> Value {
    let topic = &profile.initiatives[0];
    let mut best = if topic_evidence_allowed(topic, title) {
        classify_with_profile(profile, title, "", "")
    } else {
        classify_with_profile(profile, "", "", "")
    };
    for sentence in evidence_sentences(summary) {
        if !topic_evidence_allowed(topic, sentence) {
            continue;
        }
        let result = classify_with_profile(profile, "", "", sentence);
        if result["score"].as_u64().unwrap_or(0) > best["score"].as_u64().unwrap_or(0) {
            best = result;
        }
    }
    best
}

fn nearby_core_tokens(sentence: &str, tokens: &BTreeSet<String>) -> bool {
    let sentence = normalize(sentence);
    let positions: Vec<_> = tokens
        .iter()
        .map(|token| {
            sentence
                .match_indices(token)
                .map(|(byte, _)| sentence[..byte].chars().count())
                .collect::<Vec<_>>()
        })
        .collect();
    if positions.iter().any(Vec::is_empty) {
        return false;
    }
    // All pieces must occur in the same short span, not anywhere in one body.
    let span = tokens.iter().map(|t| t.chars().count()).sum::<usize>() + 12;
    positions.iter().flatten().any(|start| {
        positions.iter().zip(tokens).all(|(matches, token)| {
            matches.iter().any(|position| {
                *position >= *start && *position + token.chars().count() <= *start + span
            })
        })
    })
}

fn directly_linked_to_policy(
    title: &str,
    summary: &str,
    accepted: &[&Value],
    general_keywords: &[String],
) -> bool {
    let specific_terms: BTreeSet<String> = accepted
        .iter()
        .flat_map(|topic| {
            topic["matched_keywords"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        })
        .filter(|word| {
            !general_keywords
                .iter()
                .any(|general| normalize(general) == normalize(word))
        })
        .map(normalize)
        .collect();
    std::iter::once(title)
        .chain(evidence_sentences(summary))
        .map(normalize)
        .any(|sentence| {
            sentence.contains("ai新十大建設")
                && ["配合", "納入", "屬於", "項下", "列為", "落實"]
                    .iter()
                    .any(|word| sentence.contains(word))
                && specific_terms.iter().any(|term| sentence.contains(term))
        })
}

fn domain_application(topic: &Initiative, sentence: &str) -> bool {
    let sentence = normalize(sentence);
    match topic.name.as_str() {
        "百工百業智慧應用" => {
            ["農業", "農民", "農漁", "農業生產"]
                .iter()
                .any(|word| sentence.contains(word))
                && ["ai", "人工智慧"]
                    .iter()
                    .any(|word| sentence.contains(word))
                && ["感測", "自動化", "資料分析", "生產管理", "精準農業"]
                    .iter()
                    .any(|word| sentence.contains(word))
        }
        "智慧政府與資料治理" => {
            has_government_context(&sentence)
                && ["ai", "人工智慧"]
                    .iter()
                    .any(|word| sentence.contains(word))
                && ["決策輔助", "業務助手", "智慧服務", "資料匯流", "共用模組"]
                    .iter()
                    .any(|word| sentence.contains(word))
        }
        "全球量子能力登頂" => {
            sentence.contains("量子")
                && ["光電元件", "晶片", "運算", "通訊", "密碼"]
                    .iter()
                    .any(|word| sentence.contains(word))
        }
        "千億資金驅動創新" => {
            sentence.contains("國發基金")
                && ["投資", "融資", "創投", "募資"]
                    .iter()
                    .any(|word| sentence.contains(word))
        }
        _ => false,
    }
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
        for (topic, query, one) in &queries {
            let mut result = strongest_evidence(one, &item.title, &item.summary);
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
                        .filter(|&w| {
                            !profile.general_keywords.iter().any(|g| normalize(g) == *w)
                                && ![
                                    "智慧", "數位", "產業", "技術", "服務", "應用", "資料", "治理",
                                    "政府", "發展", "建設", "平台", "平臺", "人工",
                                ]
                                .contains(&w.as_str())
                        })
                        .cloned()
                        .collect();
                    if tokens.len() >= 2
                        && std::iter::once(item.title.as_str())
                            .chain(evidence_sentences(&item.summary))
                            .filter(|sentence| topic_evidence_allowed(topic, sentence))
                            .any(|sentence| nearby_core_tokens(sentence, &tokens))
                    {
                        score = profile.thresholds.possible;
                        matched.push(json!(word));
                        reasons.push(json!("核心片語斷詞全部命中，列入可能相關"));
                    }
                }
            }
            if score < profile.thresholds.possible
                && std::iter::once(item.title.as_str())
                    .chain(evidence_sentences(&item.summary))
                    .filter(|sentence| topic_evidence_allowed(topic, sentence))
                    .any(|sentence| domain_application(topic, sentence))
            {
                score = profile.thresholds.possible;
                matched.push(json!("領域、AI 與具體應用同句命中"));
                reasons.push(json!("領域與具體 AI 應用同句，列入可能相關"));
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
            // Preserve the existing manual-review lane for general AI news,
            // without inventing membership in any of the ten initiatives.
            let mut generic_profile = profile.clone();
            generic_profile.initiatives.clear();
            let lead = item.summary.chars().take(900).collect::<String>();
            classify_with_profile(&generic_profile, &item.title, &item.source, &lead)
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
        if !accepted.is_empty() {
            let reason = if directly_linked_to_policy(
                &item.title,
                &item.summary,
                &accepted,
                &profile.general_keywords,
            ) {
                "政策直接關聯"
            } else {
                "政策間接關聯"
            };
            result["reasons"]
                .as_array_mut()
                .unwrap()
                .push(json!(reason));
        }
        batch.results.push(result);
    }
    batch
}
