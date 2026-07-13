#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from news_scraper.ai_policy_evaluation import (
    evaluate_ai_policy_corpus,
    load_labeled_ai_policy_corpus,
    validate_labeled_ai_policy_corpus,
    verify_corpus_source_titles,
)


def main():
    parser = argparse.ArgumentParser(description="評估 AI 新十大建設規則的 precision/recall。")
    parser.add_argument(
        "corpus",
        nargs="?",
        default="tests/fixtures/ai_policy_labeled_titles.tsv",
        help="人工標註 TSV 路徑",
    )
    parser.add_argument("--require-published-date", action="store_true", help="要求每列具有 ISO 發布日期")
    parser.add_argument("--verify-sources", action="store_true", help="連線至來源網址並確認頁面包含標題")
    args = parser.parse_args()
    rows = load_labeled_ai_policy_corpus(Path(args.corpus))
    validate_labeled_ai_policy_corpus(rows, require_published_date=args.require_published_date)
    metrics = evaluate_ai_policy_corpus(rows)
    if args.verify_sources:
        metrics["source_verification"] = verify_corpus_source_titles(rows)
    print(json.dumps(metrics, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
