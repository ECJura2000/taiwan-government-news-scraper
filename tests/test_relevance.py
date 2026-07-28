import json

import pytest

from news_scraper.relevance import (
    ExclusionRule,
    KeywordRule,
    RelevanceProfile,
    TopicRule,
    build_default_relevance_profile,
    classify_relevance,
    compare_relevance_profiles,
    delete_default_aware,
    get_effective_relevance_hash,
    load_relevance_profile,
    merge_new_default_rules,
    new_custom_id,
    save_relevance_profile,
    validate_relevance_profile,
)


def build_cyber_profile():
    return RelevanceProfile(
        name="通用測試",
        include_unassigned_context_matches=False,
        global_context_keywords=[
            KeywordRule(id="context-security", text="資安"),
        ],
        topics=[
            TopicRule(
                id="topic-zero-trust",
                name="零信任架構",
                priority_sources=["數位發展部", "國防部"],
                core_keywords=[
                    KeywordRule(id="core-zero-trust", text="zero trust"),
                ],
                supporting_keywords=[
                    KeywordRule(id="support-identity", text="身分驗證"),
                ],
                context_keywords=[
                    KeywordRule(id="context-access", text="存取控制"),
                ],
            ),
            TopicRule(
                id="topic-supply-chain",
                name="供應鏈安全",
                match_name=False,
                core_keywords=[
                    KeywordRule(id="core-sbom", text="SBOM"),
                ],
            ),
        ],
    )


def test_default_template_preserves_existing_ai_scores():
    profile = build_default_relevance_profile()

    exact = classify_relevance(
        "主權AI及算力建設正式啟動",
        source="國科會",
        profile=profile,
    )
    contextual = classify_relevance(
        "協助中小微型企業導入AI加速轉型",
        source="經濟部",
        profile=profile,
    )
    summary = classify_relevance(
        "前瞻技術計畫",
        source="經濟部",
        summary="將建構矽光子技術全球領先優勢。",
        profile=profile,
    )

    assert (exact["relevance"], exact["score"]) == ("高度相關", 100)
    assert (contextual["relevance"], contextual["score"]) == ("高度相關", 80)
    assert (summary["relevance"], summary["score"]) == ("可能相關", 70)


def test_custom_non_ai_topic_uses_fixed_explainable_scoring():
    profile = build_cyber_profile()

    exact = classify_relevance(
        "政府推動零信任架構",
        source="文化部",
        profile=profile,
    )
    core = classify_relevance(
        "導入 Zero Trust 強化防護",
        source="文化部",
        profile=profile,
    )
    supporting = classify_relevance(
        "資安身分驗證制度上線",
        source="數位發展部",
        profile=profile,
    )

    assert (exact["topics"], exact["score"]) == (["零信任架構"], 100)
    assert (core["topics"], core["score"]) == (["零信任架構"], 85)
    assert (supporting["topics"], supporting["score"]) == (["零信任架構"], 80)


def test_global_and_topic_exclusions_apply_once_and_respect_fields():
    profile = build_cyber_profile()
    profile.exclusions.extend(
        [
            ExclusionRule(
                id="global-job",
                text="徵才",
                match_fields=["title"],
            ),
            ExclusionRule(
                id="topic-course",
                text="課程",
                topic_id="topic-zero-trust",
                match_fields=["summary"],
            ),
        ]
    )

    result = classify_relevance(
        "零信任架構徵才公告",
        source="數位發展部",
        summary="本課程介紹零信任架構。",
        profile=profile,
    )

    assert result["score"] == 50
    assert result["relevance"] == "可能相關"
    assert result["excluded_keywords"] == ["徵才", "課程"]


def test_topic_exclusion_does_not_affect_other_topics():
    profile = build_cyber_profile()
    profile.exclusions.append(
        ExclusionRule(
            id="topic-course",
            text="課程",
            topic_id="topic-zero-trust",
            match_fields=["title"],
        )
    )

    result = classify_relevance(
        "SBOM 課程介紹",
        profile=profile,
    )

    assert result["topics"] == ["供應鏈安全"]
    assert result["score"] == 85


def test_below_threshold_candidate_keeps_score_topic_and_exclusion_reason():
    profile = build_cyber_profile()
    profile.exclusions.append(
        ExclusionRule(
            id="global-job",
            text="徵才",
            match_fields=["title"],
        )
    )

    result = classify_relevance(
        "Zero Trust 徵才公告",
        source="文化部",
        profile=profile,
    )

    assert result["relevance"] == "未納入"
    assert result["score"] == 35
    assert result["topics"] == ["零信任架構"]
    assert result["excluded_keywords"] == ["徵才"]
    assert result["topic_matches"][0]["relevance"] == "未納入"
    assert "命中排除詞，分數下修" in result["reasons"][1]


def test_profile_round_trip_and_effective_hash(tmp_path):
    profile = build_cyber_profile()
    path = tmp_path / "relevance-profile.json"

    save_relevance_profile(path, profile)
    loaded = load_relevance_profile(path, merge_defaults=False)

    assert loaded.profile.to_dict() == profile.to_dict()
    assert get_effective_relevance_hash(loaded.profile) == get_effective_relevance_hash(
        profile
    )
    assert json.loads(path.read_text(encoding="utf-8"))["schema_version"] == 1


def test_pure_custom_profile_does_not_gain_ai_template_topics_on_load(tmp_path):
    profile = build_cyber_profile()
    path = tmp_path / "relevance-profile.json"
    save_relevance_profile(path, profile)

    loaded = load_relevance_profile(default_path=path)

    assert [topic.name for topic in loaded.profile.topics] == [
        "零信任架構",
        "供應鏈安全",
    ]
    assert loaded.profile.template_version == ""


def test_deleted_default_keyword_stays_deleted_when_new_defaults_merge():
    defaults = build_default_relevance_profile()
    profile = defaults.clone()
    topic = profile.topics[0]
    removed = topic.supporting_keywords.pop(0)
    delete_default_aware(profile, removed.id)

    merged, _changed = merge_new_default_rules(profile, defaults)

    assert removed.id not in {rule.id for rule in merged.topics[0].supporting_keywords}


def test_new_default_keyword_is_added_without_losing_custom_items():
    defaults = build_default_relevance_profile()
    profile = defaults.clone()
    custom = KeywordRule(id=new_custom_id("keyword"), text="自訂詞")
    profile.topics[0].core_keywords.append(custom)
    updated_defaults = defaults.clone()
    updated_defaults.topics[0].core_keywords.append(
        KeywordRule(id="default:core:new", text="新版預設詞", origin="default")
    )

    merged, changed = merge_new_default_rules(profile, updated_defaults)

    ids = {rule.id for rule in merged.topics[0].core_keywords}
    assert changed is True
    assert custom.id in ids
    assert "default:core:new" in ids


def test_validation_rejects_include_exclude_conflict_and_empty_topics():
    profile = build_cyber_profile()
    profile.exclusions.append(
        ExclusionRule(id="exclude-sbom", text="SBOM", match_fields=["title"])
    )

    with pytest.raises(ValueError, match="同時設為納入與排除"):
        validate_relevance_profile(profile)

    for topic in profile.topics:
        topic.enabled = False
    with pytest.raises(ValueError, match="至少需要一個啟用的主題"):
        validate_relevance_profile(profile)


def test_profile_comparison_reports_added_modified_and_removed_items():
    current = RelevanceProfile(
        name="目前設定",
        topics=[
            TopicRule(
                id="topic:one",
                name="產業政策",
                core_keywords=[KeywordRule(id="keyword:old", text="半導體")],
            )
        ],
    )
    incoming = RelevanceProfile(
        name="匯入設定",
        topics=[
            TopicRule(
                id="topic:one",
                name="產業政策與投資",
                core_keywords=[KeywordRule(id="keyword:new", text="先進製程")],
            )
        ],
    )

    difference = compare_relevance_profiles(current, incoming)

    assert difference["added_count"] == 1
    assert difference["modified_count"] == 2
    assert difference["removed_count"] == 1
    assert difference["added"] == ["核心詞「先進製程」的「產業政策與投資」"]
    assert "設定屬性" in difference["modified"]
    assert "主題「產業政策與投資」" in difference["modified"]
    assert difference["removed"] == ["核心詞「半導體」的「產業政策」"]
