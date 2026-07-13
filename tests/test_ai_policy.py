import pytest

from news_scraper.config import (
    AIPolicyInitiative,
    AI_POLICY_INITIATIVES,
    get_ai_policy_ruleset_hash,
    validate_ai_policy_initiatives,
)
from news_scraper.utils.text import classify_ai_policy_relevance, title_matches_ai_policy_keywords


@pytest.mark.parametrize(
    ("title", "source", "expected_initiative"),
    [
        ("推動全民智慧生活圈示範計畫", "國科會", "全民智慧生活圈"),
        ("協助百工百業智慧應用加速落地", "經濟部", "百工百業智慧應用"),
        ("AI數位產業登峰計畫正式啟動", "數位發展部", "AI數位產業登峰"),
        ("矽光子技術全球領先布局", "經濟部", "矽光子技術全球領先"),
        ("全球量子能力登頂研發計畫", "國科會", "全球量子能力登頂"),
        ("打造全球AI機器人供應鏈樞紐", "國科會", "全球AI機器人供應鏈樞紐"),
        ("主權AI及算力建設啟用", "國科會", "主權AI及算力建設"),
        ("智慧政府與資料治理新里程", "數位發展部", "智慧政府與資料治理"),
        ("AI人才生態系引領國際", "國發會", "AI人才生態系引領國際"),
        ("千億資金驅動創新方案", "國發會", "千億資金驅動創新"),
    ],
)
def test_all_ten_ai_policy_initiatives_are_classified(title, source, expected_initiative):
    result = classify_ai_policy_relevance(title, source=source)

    assert result["relevance"] == "高度相關"
    assert expected_initiative in result["initiatives"]


def test_ai_policy_configuration_has_exactly_ten_unique_initiatives():
    names = [initiative.name for initiative in AI_POLICY_INITIATIVES]

    assert len(names) == len(set(names)) == 10
    assert len(get_ai_policy_ruleset_hash()) == 16


def test_generic_ai_title_is_only_possible_and_requires_manual_review():
    result = classify_ai_policy_relevance("機關辦理AI應用成果交流會", source="文化部")

    assert result["relevance"] == "可能相關"
    assert result["initiatives"] == ["待人工判讀"]


def test_broad_legacy_words_no_longer_trigger_false_positives():
    for title in ["醫療服務品質提升", "建築管理法規修正", "強化內部控制", "提升城市韌性"]:
        assert title_matches_ai_policy_keywords(title) is False


def test_context_keyword_with_ai_and_lead_agency_is_high_relevance():
    result = classify_ai_policy_relevance("協助中小微型企業導入AI加速轉型", source="經濟部")

    assert result["relevance"] == "高度相關"
    assert result["score"] == 80
    assert result["initiatives"] == ["百工百業智慧應用"]


def test_summary_only_match_is_possible_not_high_relevance():
    result = classify_ai_policy_relevance(
        "推動產業創新方案",
        source="經濟部",
        summary="計畫將建構矽光子技術全球領先優勢。",
    )

    assert result["relevance"] == "可能相關"
    assert result["score"] == 70
    assert result["initiatives"] == ["矽光子技術全球領先"]
    assert "摘要命中完整建設名稱" in result["reasons"][0]


def test_negative_keyword_prevents_recruitment_false_positive():
    result = classify_ai_policy_relevance("AI人才招募公告", source="文化部")

    assert result["relevance"] == ""
    assert result["score"] == 0
    assert result["negative_keywords"] == ["招募"]


def test_each_initiative_keeps_its_own_score_and_relevance():
    result = classify_ai_policy_relevance(
        "主權AI及算力建設正式啟動",
        source="國科會",
        summary="後續也將推動矽光子技術全球領先驗證場域。",
    )
    matches = {match["name"]: match for match in result["initiative_matches"]}

    assert result["score"] == 100
    assert matches["主權AI及算力建設"]["relevance"] == "高度相關"
    assert matches["主權AI及算力建設"]["score"] == 100
    assert matches["矽光子技術全球領先"]["relevance"] == "可能相關"
    assert matches["矽光子技術全球領先"]["score"] == 70


def test_ai_policy_configuration_validation_rejects_unknown_lead_source():
    invalid = AIPolicyInitiative(
        name="測試建設",
        lead_agency="不存在部會",
        lead_source="不存在部會",
        exact_phrases=("測試建設",),
        strong_keywords=("測試精準詞",),
        context_keywords=("測試輔助詞",),
    )

    with pytest.raises(RuntimeError, match="主政來源不存在"):
        validate_ai_policy_initiatives((invalid,))
