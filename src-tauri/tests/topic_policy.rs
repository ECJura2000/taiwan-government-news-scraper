use serde_json::{json, Value};
use taiwan_government_news_lib::{
    policy::{self, Profile},
    ranking,
    scraper::NewsItem,
};
fn news(title: &str, summary: &str) -> NewsItem {
    serde_json::from_value(json!({"source":"測試機關","date":"2026-08-31","title":title,"summary":summary,"link":format!("https://example.org/{title}"),"department":"測試機關","category":"新聞","date_source":"published"})).unwrap()
}
fn profile() -> Profile {
    Profile::parse(r#"{"initiatives":[{"name":"甲","strong_keywords":["智慧醫療"],"penalty_keywords":[{"text":"研討會","penalty":20},{"text":"活動","penalty":10}],"exclude_keywords":[{"text":"徵才"}]},{"name":"乙","strong_keywords":["量子運算"]}]}"#).unwrap()
}
#[test]
fn exclusions_are_local_and_penalties_take_max_once() {
    let p = profile();
    let out = ranking::rank(
        &p,
        &[
            news("智慧醫療徵才", ""),
            news("智慧醫療徵才與量子運算", ""),
            news("智慧醫療研討會活動研討會", ""),
            news("普通新聞", ""),
        ],
    );
    assert_eq!(out.excluded_count, 1);
    assert_eq!(out.results[0]["hard_excluded"], true);
    assert_eq!(out.results[1]["hard_excluded"], false);
    assert_eq!(out.results[1]["topics"], json!(["乙"]));
    assert_eq!(out.results[2]["score"], 65);
    assert_eq!(out.results[3]["hard_excluded"], false);
    assert_eq!(out.topic_counts["甲"], 1);
    assert_eq!(out.topic_counts["乙"], 1);
}
#[test]
fn disabled_rules_and_topics_do_not_apply() {
    let mut p = profile();
    p.initiatives[0].exclude_keywords[0].enabled = false;
    assert_eq!(
        ranking::rank(&p, &[news("智慧醫療徵才", "")]).excluded_count,
        0
    );
    p.initiatives[0].enabled = false;
    let out = ranking::rank(&p, &[news("智慧醫療徵才", "")]);
    assert_eq!(out.excluded_count, 0);
    assert!(out.results[0]["topics"].as_array().unwrap().is_empty());
}
#[test]
fn field_boost_and_english_boundaries() {
    let p = profile();
    let out = ranking::rank(
        &p,
        &[
            news("智慧醫療", "普通消息"),
            news("普通消息", "智慧醫療"),
            news("普通消息", "一般消息"),
        ],
    );
    let title = out.results[0]["topic_matches"][0]["bm25_score"]
        .as_f64()
        .unwrap();
    let summary = out.results[1]["topic_matches"][0]["bm25_score"]
        .as_f64()
        .unwrap();
    assert!(title > summary, "title={title} summary={summary}");
    let p =
        Profile::parse(r#"{"initiatives":[{"name":"模型","strong_keywords":["LLM"]}]}"#).unwrap();
    let r = ranking::rank(&p, &[news("LLM system", ""), news("smallmodel", "")]);
    assert_eq!(r.results[0]["relevance"], "高度相關");
    assert!(r.results[1]["topics"].as_array().unwrap().is_empty());
}
#[test]
fn empty_singleton_and_order_are_deterministic() {
    let p = profile();
    assert!(ranking::rank(&p, &[]).results.is_empty());
    let single = ranking::rank(&p, &[news("智慧醫療", "")]);
    assert!(single.results[0]["bm25_score"]
        .as_f64()
        .unwrap()
        .is_finite());
    let a = news("智慧醫療", "");
    let b = news("量子運算", "");
    let x = ranking::rank(&p, &[a.clone(), b.clone()]);
    let y = ranking::rank(&p, &[b, a]);
    assert_eq!(x.results[0], y.results[1]);
}
#[test]
fn merge_replace_validation_and_persistence() {
    let p = profile();
    let incoming = r#"{"initiatives":[{"name":"甲","strong_keywords":["新版"]},{"name":"丙","strong_keywords":["第三"]}]}"#;
    let v = policy::preview_import(&p, incoming, false).unwrap();
    assert_eq!(v["added"], json!(["丙"]));
    assert_eq!(v["updated"], json!(["甲"]));
    assert_eq!(v["profile"]["initiatives"].as_array().unwrap().len(), 3);
    assert_eq!(
        v["profile"]["initiatives"][0]["exclude_keywords"],
        json!([])
    );
    let v = policy::preview_import(&p, incoming, true).unwrap();
    assert_eq!(v["deleted"], json!(["乙"]));
    assert!(Profile::parse(r#"{"initiatives":[{"name":"ＡＩ"},{"name":"ai"}]}"#).is_err());
    assert!(Profile::parse(r#"{"initiatives":[{"name":"甲","exclude_keywords":[{"text":"","match_fields":["body"]}]}]}"#).is_err());
    assert!(Profile::parse(r#"{"schema_version":9,"initiatives":[]}"#).is_err());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topics.json");
    policy::save_profile(&path, &p).unwrap();
    let mut invalid = p.clone();
    invalid.thresholds.possible = 200;
    assert!(policy::save_profile(&path, &invalid).is_err());
    assert_eq!(policy::read_profile(&path).unwrap().hash(), p.hash());
    let mut changed = p.clone();
    changed.scoring.title_weight = 4.0;
    assert_ne!(changed.hash(), p.hash());
}
#[test]
fn all_examples_and_pdf_provenance_are_complete() {
    for text in [
        include_str!("../../examples/topics/ai-ten-topics.json"),
        include_str!("../../examples/topics/two-topics.json"),
    ] {
        Profile::parse(text).unwrap().require_enabled().unwrap();
    }
    let p = Profile::embedded();
    assert_eq!(p.initiatives.len(), 10);
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        serde_json::to_value(
            Profile::parse(include_str!("../../examples/topics/ai-ten-topics.json")).unwrap()
        )
        .unwrap()
    );
    for topic in &p.initiatives {
        assert!(topic.weighted_keywords.iter().any(|w| w.origin == "pdf"));
        for word in &topic.weighted_keywords {
            assert!(!word.references.is_empty());
            if word.origin == "pdf" {
                assert!(!word.references[0]["pages"].as_array().unwrap().is_empty());
            }
        }
    }
}
#[test]
fn new_policy_recall_and_fixture_precision_do_not_regress() {
    let p = Profile::embedded();
    let fixtures = include_str!("fixtures/relevance_labeled.tsv");
    let rows: Vec<_> = fixtures
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect();
    let items: Vec<_> = rows
        .iter()
        .map(|c| {
            let mut n = news(c[1], c[2]);
            n.source = c[0].into();
            n
        })
        .collect();
    let out = ranking::rank(&p, &items);
    let mut tp = 0;
    let mut fp = 0;
    let mut high_tp = 0;
    let mut high_total = 0;
    let mut missed = vec![];
    for (i, (r, c)) in out.results.iter().zip(&rows).enumerate() {
        let positive = r["relevance"] == "高度相關" || r["relevance"] == "可能相關";
        if r["relevance"] == "高度相關" {
            high_total += 1;
            if !c[3].is_empty() {
                high_tp += 1;
            }
        }
        if !c[3].is_empty() {
            if positive {
                tp += 1;
            } else {
                missed.push(i + 2);
            }
        } else if positive {
            fp += 1;
        }
    }
    eprintln!(
        "policy evaluation: TP={tp}, FP={fp}, FN={}, high_precision={high_tp}/{high_total}, total={}",
        missed.len(),
        rows.len()
    );
    assert!(missed.is_empty(), "lost positive rows: {missed:?}");
    assert_eq!(fp, 0, "introduced false positives");
    assert!(high_total > 0 && high_tp * 10 >= high_total * 9);
    let legacy = Profile::parse(include_str!("fixtures/legacy-policy.json")).unwrap();
    for title in ["TAICA 推動課程共享", "CryoCMOS 技術研發"] {
        let before = taiwan_government_news_lib::relevance::classify_with_profile(
            &legacy,
            title,
            "測試機關",
            "",
        );
        assert!(before["score"].as_u64().unwrap_or(0) < 40);
    }
    let out = ranking::rank(
        &p,
        &[
            news("TAICA 推動課程共享", ""),
            news("CryoCMOS 技術研發", ""),
        ],
    );
    assert!(out
        .results
        .iter()
        .all(|r| r["score"].as_u64().unwrap_or(0) >= 40));
}
#[test]
fn partial_scoring_preview_shows_effective_defaults() {
    let mut p = profile();
    p.scoring.title_weight = 8.0;
    let v = policy::preview_import(&p, r#"{"initiatives":[],"scoring":{"b":0.5}}"#, false).unwrap();
    assert_eq!(v["profile"]["scoring"]["title_weight"], 3.0);
    assert!(!v["common_changes"].as_array().unwrap().is_empty());
    let _: Value = serde_json::to_value(p).unwrap();
}

#[test]
fn separated_core_tokens_recall_without_literal_phrase() {
    let p = Profile::parse(r#"{"initiatives":[{"name":"科研","strong_keywords":["量子運算"]}]}"#)
        .unwrap();
    let result = ranking::rank(&p, &[news("量子技術與運算系統整合", "")]);
    assert_eq!(result.results[0]["relevance"], "可能相關");
    assert_eq!(result.results[0]["topics"], json!(["科研"]));
    let too_far = ranking::rank(
        &p,
        &[news(
            "量子與前瞻基礎探測技術及多元測試場域共同發展運算系統",
            "",
        )],
    );
    assert!(
        too_far.results[0]["topics"].as_array().unwrap().is_empty(),
        "{}",
        too_far.results[0]
    );
}

#[test]
fn policy_examples_require_concrete_applications_and_separate_topics() {
    let p = Profile::embedded();
    let cases = [
        news(
            "不作秀只解痛！環境部AI轉型挺第一線",
            "政府推動AI不是為了秀技術，而是讓它當業務助手，改善第一線環境治理工作。",
        ),
        news(
            "2026生技產業策略諮議委員會開幕 卓揆：加速AI、數位醫療與生技產業發展",
            "政府編列406億元推動AI新十大建設，並期待數位醫療服務落地。",
        ),
        news(
            "關務智慧領航計畫將發展兩項關務智慧服務",
            "關務署導入代理式人工智慧，發展通關智慧特助與貨物分類智慧助理。",
        ),
        news(
            "行政院2026 BTC會議圓滿閉幕 成立智慧生技國家隊",
            "以AI推動數位醫療服務與疾病早篩。另以智慧農業2.0導入AI感測及自動化農業生產管理。",
        ),
        news(
            "臺灣在APEC分享智慧農業2.0成果 以AI科技強化區域糧食安全韌性",
            "農業部分享AI、感測與自動化技術提升農業生產效率。",
        ),
    ];
    let results = ranking::rank(&p, &cases).results;
    assert_eq!(results[0]["topics"], json!(["智慧政府與資料治理"]));
    assert_eq!(results[1]["topics"], json!(["全民智慧生活圈"]));
    assert_eq!(results[2]["topics"], json!(["智慧政府與資料治理"]));
    assert_eq!(
        results[3]["topics"],
        json!(["全民智慧生活圈", "百工百業智慧應用"])
    );
    assert_eq!(results[4]["topics"], json!(["百工百業智慧應用"]));
    assert!(results.iter().all(|result| {
        result["topics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|topic| topic != "千億資金驅動創新")
    }));
    assert!(results[4]["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("政策間接關聯")));
}

#[test]
fn distant_or_generic_words_do_not_create_formal_topics() {
    let p = Profile::embedded();
    let cases = [
        news("AI新十大建設預算編列406億元", "政府說明整體計畫預算。"),
        news("人工智慧新聞", "介紹人工智慧發展趨勢。"),
        news("一般政策消息", "第一段談量子。第二段談行政運算系統。"),
        news(
            "健康資料治理模式取得進展",
            "醫療資料互通可提升智慧醫療服務。",
        ),
    ];
    let results = ranking::rank(&p, &cases).results;
    for result in &results[..3] {
        assert!(result["topics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|topic| topic == "待人工判讀"));
    }
    assert_eq!(results[3]["topics"], json!(["全民智慧生活圈"]));
    let mut source_only = news("國發基金發布消息", "介紹近期工作。");
    source_only.source = "國發會".into();
    let source_result = ranking::rank(&p, &[source_only]);
    assert!(source_result.results[0]["topics"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn later_body_mentions_do_not_add_a_second_topic() {
    let p = Profile::embedded();
    let summary = format!(
        "AI數位醫療服務將於醫院導入。{}智慧農業2.0計畫也在另一段被提及。",
        "一般背景資料。".repeat(100)
    );
    let result = &ranking::rank(&p, &[news("AI數位醫療服務上線", &summary)]).results[0];
    assert_eq!(result["topics"], json!(["全民智慧生活圈"]));
}

#[test]
fn direct_policy_reason_requires_a_linked_measure_in_the_same_sentence() {
    let p = Profile::embedded();
    let direct = ranking::rank(
        &p,
        &[news(
            "智慧農業2.0成果",
            "農業部配合AI新十大建設推動智慧農業2.0，導入AI感測設備。",
        )],
    );
    assert!(direct.results[0]["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("政策直接關聯")));
    let indirect = ranking::rank(
        &p,
        &[news(
            "智慧農業2.0成果",
            "農業部推動智慧農業2.0。政府另編列AI新十大建設總預算。",
        )],
    );
    assert!(indirect.results[0]["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("政策間接關聯")));
}

#[test]
fn penalty_keeps_below_threshold_rule_score() {
    let mut p = profile();
    p.initiatives[0].penalty_keywords[0].penalty = 50;
    let out = ranking::rank(&p, &[news("智慧醫療研討會", "")]);
    assert_eq!(out.results[0]["score"], 35);
    assert_eq!(out.results[0]["relevance"], "未納入");
    assert!(out.results[0]["bm25_score"].as_f64().unwrap() > 0.0);
    assert_eq!(out.excluded_count, 0);
}

/// Live parity gate; opt in explicitly, because the official feed requires network access.
#[tokio::test]
#[ignore = "requires the live 國發會 RSS feed"]
async fn live_cli_file_and_gui_snapshot_produce_same_news() {
    use std::sync::{atomic::AtomicBool, Arc};
    use taiwan_government_news_lib::{native, RunOptions};
    let dir = tempfile::tempdir().unwrap();
    let profile = Profile::embedded();
    let path = dir.path().join("topics.json");
    policy::save_profile(&path, &profile).unwrap();
    let mut options = RunOptions {
        sources: vec!["國發會".into()],
        topics_json: Some(path.to_string_lossy().into_owned()),
        topics_policy: None,
        output_dir: Some(dir.path().join("cli").to_string_lossy().into_owned()),
        report_dir: None,
        date: Some("2026-09-09".into()),
        start_date: None,
        end_date: None,
        max_workers: 1,
        dedupe_affiliated: false,
        fail_on_source_error: true,
    };
    let cli = native::run(&options, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    options.topics_json = None;
    options.topics_policy = Some(profile);
    options.output_dir = Some(dir.path().join("gui").to_string_lossy().into_owned());
    let gui = native::run(&options, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    let a = serde_json::to_value(&cli).unwrap();
    let b = serde_json::to_value(&gui).unwrap();
    assert_eq!(a["status"], "success");
    assert_eq!(b["status"], "success");
    for key in [
        "news_count",
        "source_counts",
        "relevance_policy",
        "quality",
        "week_start",
        "week_end",
    ] {
        assert_eq!(a[key], b[key], "entry point mismatch: {key}");
    }
    use std::io::Read;
    let read = |path: &str, sheet: &str| {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut xml = String::new();
        zip.by_name(sheet)
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        xml
    };
    for n in 1..=12 {
        let sheet = format!("xl/worksheets/sheet{n}.xml");
        assert_eq!(
            read(a["output_file"].as_str().unwrap(), &sheet),
            read(b["output_file"].as_str().unwrap(), &sheet)
        );
    }
    eprintln!(
        "live CLI/GUI parity: {} news, 12 news sheets identical",
        cli.news_count
    );
}
