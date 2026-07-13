from pathlib import Path

from news_scraper.ai_policy_evaluation import (
    evaluate_ai_policy_corpus,
    load_labeled_ai_policy_corpus,
    validate_labeled_ai_policy_corpus,
    verify_corpus_source_titles,
)

CORPUS_PATH = Path(__file__).parent / "fixtures" / "ai_policy_labeled_titles.tsv"
HOLDOUT_PATH = Path(__file__).parent / "fixtures" / "ai_policy_holdout_20260622.tsv"


def test_real_historical_corpus_meets_precision_and_recall_budgets():
    rows = load_labeled_ai_policy_corpus(CORPUS_PATH)
    validate_labeled_ai_policy_corpus(rows)
    metrics = evaluate_ai_policy_corpus(rows)

    assert metrics["row_count"] >= 200
    assert all(row["source_url"].startswith(("http://", "https://")) for row in rows)
    assert metrics["detection"]["precision"] >= 0.90
    assert metrics["detection"]["recall"] >= 0.90
    assert metrics["exact_relevance_accuracy"] >= 0.90

    for initiative_metrics in metrics["initiatives"].values():
        assert initiative_metrics["support"] >= 4
        assert initiative_metrics["precision"] >= 0.85
        assert initiative_metrics["recall"] >= 0.85
        assert initiative_metrics["relevance_accuracy"] >= 0.85


def test_frozen_temporal_holdout_is_disjoint_and_meets_quality_budget():
    training_rows = load_labeled_ai_policy_corpus(CORPUS_PATH)
    holdout_rows = load_labeled_ai_policy_corpus(HOLDOUT_PATH)
    validate_labeled_ai_policy_corpus(holdout_rows, require_published_date=True)
    metrics = evaluate_ai_policy_corpus(holdout_rows)

    assert len(holdout_rows) >= 30
    assert {row["title"] for row in training_rows}.isdisjoint(row["title"] for row in holdout_rows)
    assert metrics["detection"]["precision"] >= 0.90
    assert metrics["detection"]["recall"] >= 0.90
    assert metrics["exact_relevance_accuracy"] >= 0.90
    assert metrics["mismatches"] == []


def test_source_title_verifier_checks_page_content():
    rows = [
        {
            "title": "主權AI及算力建設正式啟動",
            "source_url": "https://example.gov.tw/news/1",
        },
        {
            "title": "頁面不存在的標題",
            "source_url": "https://example.gov.tw/news/2",
        },
    ]
    pages = {
        "https://example.gov.tw/news/1": "<html><h1>主權 AI 及算力建設正式啟動</h1></html>",
        "https://example.gov.tw/news/2": "<html><h1>其他新聞</h1></html>",
    }

    audit = verify_corpus_source_titles(rows, fetcher=pages.__getitem__)

    assert audit["verified_count"] == 1
    assert audit["title_not_found_count"] == 1
    assert audit["fetch_failed_count"] == 0


def test_empty_metric_denominators_are_reported_as_undefined():
    metrics = evaluate_ai_policy_corpus([])

    assert metrics["detection"]["precision"] is None
    assert metrics["detection"]["recall"] is None
    assert metrics["initiatives"]["全民智慧生活圈"]["relevance_accuracy"] is None
