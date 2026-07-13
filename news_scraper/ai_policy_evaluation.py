import csv
from collections import defaultdict
from datetime import date
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse

from .config import AI_POLICY_INITIATIVES
from .http.client import fetch_html_resilient
from .utils.text import classify_ai_policy_relevance, normalize_keyword_match_text

CORPUS_REQUIRED_COLUMNS = {
    "source",
    "title",
    "summary",
    "expected_relevance",
    "expected_initiative",
    "source_url",
}
OFFICIAL_SOURCE_HOST_SUFFIXES = (".gov.tw", ".edu.tw")
OFFICIAL_SOURCE_HOSTS = {"indsr.org.tw", "www.ncsist.org.tw", "www.taiwan.net.tw", "www.tasa.org.tw"}
VALID_RELEVANCE_LABELS = {"", "可能相關", "高度相關"}


def load_labeled_ai_policy_corpus(path):
    with Path(path).open("r", encoding="utf-8", newline="") as corpus_file:
        return list(csv.DictReader(corpus_file, delimiter="\t"))


def validate_labeled_ai_policy_corpus(rows, *, require_published_date=False):
    errors = []
    known_initiatives = {initiative.name for initiative in AI_POLICY_INITIATIVES} | {"待人工判讀"}
    seen_titles = set()

    for row_number, row in enumerate(rows, 2):
        missing_columns = CORPUS_REQUIRED_COLUMNS - set(row)
        if missing_columns:
            errors.append("第 {} 列缺少欄位：{}".format(row_number, ", ".join(sorted(missing_columns))))
            continue

        title = row.get("title", "").strip()
        if not title:
            errors.append("第 {} 列標題空白".format(row_number))
        elif title in seen_titles:
            errors.append("第 {} 列標題重複：{}".format(row_number, title))
        seen_titles.add(title)

        relevance = row.get("expected_relevance", "").strip()
        if relevance not in VALID_RELEVANCE_LABELS:
            errors.append("第 {} 列關聯性標籤無效：{}".format(row_number, relevance))

        initiatives = {name.strip() for name in row.get("expected_initiative", "").split("|") if name.strip()}
        unknown_initiatives = initiatives - known_initiatives
        if unknown_initiatives:
            errors.append("第 {} 列建設標籤無效：{}".format(row_number, ", ".join(sorted(unknown_initiatives))))
        if bool(relevance) != bool(initiatives):
            errors.append("第 {} 列關聯性與建設標籤不一致".format(row_number))

        source_url = row.get("source_url", "").strip()
        parsed_url = urlparse(source_url)
        host = (parsed_url.hostname or "").lower()
        if parsed_url.scheme not in {"http", "https"} or not host:
            errors.append("第 {} 列來源網址無效：{}".format(row_number, source_url))
        elif not host.endswith(OFFICIAL_SOURCE_HOST_SUFFIXES) and host not in OFFICIAL_SOURCE_HOSTS:
            errors.append("第 {} 列不是核准的官方來源網域：{}".format(row_number, host))

        if require_published_date:
            published_date = row.get("published_date", "").strip()
            try:
                date.fromisoformat(published_date)
            except ValueError:
                errors.append("第 {} 列發布日期無效：{}".format(row_number, published_date))

    if errors:
        raise ValueError("AI 政策標記語料驗證失敗：{}".format("；".join(errors)))
    return rows


def verify_corpus_source_titles(
    rows,
    *,
    fetcher: Callable[[str], str] | None = None,
):
    fetcher = fetcher or fetch_html_resilient
    results = []
    for row in rows:
        title = row.get("title", "").strip()
        source_url = row.get("source_url", "").strip()
        try:
            page_text = fetcher(source_url)
            found = normalize_keyword_match_text(title) in normalize_keyword_match_text(page_text)
            results.append(
                {
                    "source_url": source_url,
                    "title": title,
                    "status": "verified" if found else "title_not_found",
                }
            )
        except Exception as exc:
            results.append(
                {
                    "source_url": source_url,
                    "title": title,
                    "status": "fetch_failed",
                    "error": str(exc),
                }
            )
    return {
        "row_count": len(rows),
        "verified_count": sum(result["status"] == "verified" for result in results),
        "title_not_found_count": sum(result["status"] == "title_not_found" for result in results),
        "fetch_failed_count": sum(result["status"] == "fetch_failed" for result in results),
        "results": results,
    }


def calculate_binary_metrics(true_positive, false_positive, false_negative):
    precision_denominator = true_positive + false_positive
    recall_denominator = true_positive + false_negative
    return {
        "true_positive": true_positive,
        "false_positive": false_positive,
        "false_negative": false_negative,
        "precision": true_positive / precision_denominator if precision_denominator else None,
        "recall": true_positive / recall_denominator if recall_denominator else None,
    }


def evaluate_ai_policy_corpus(rows):
    detection_counts: defaultdict[str, int] = defaultdict(int)
    initiative_counts: dict[str, defaultdict[str, int]] = {
        initiative.name: defaultdict(int)
        for initiative in AI_POLICY_INITIATIVES
    }
    initiative_relevance_matches: defaultdict[str, int] = defaultdict(int)
    exact_relevance_matches = 0
    mismatches = []

    for row in rows:
        result = classify_ai_policy_relevance(
            row.get("title", ""),
            source=row.get("source", ""),
            summary=row.get("summary", ""),
        )
        expected_relevance = row.get("expected_relevance", "").strip()
        expected_initiatives = {
            name.strip()
            for name in row.get("expected_initiative", "").split("|")
            if name.strip() and name.strip() != "待人工判讀"
        }
        predicted_initiatives = {
            name
            for name in result["initiatives"]
            if name != "待人工判讀"
        }
        predicted_match_map = {
            match["name"]: match
            for match in result["initiative_matches"]
            if match["name"] != "待人工判讀"
        }
        expected_detected = bool(expected_relevance)
        predicted_detected = bool(result["relevance"])

        if expected_detected and predicted_detected:
            detection_counts["true_positive"] += 1
        elif predicted_detected:
            detection_counts["false_positive"] += 1
        elif expected_detected:
            detection_counts["false_negative"] += 1

        if expected_relevance == result["relevance"]:
            exact_relevance_matches += 1

        has_per_initiative_relevance_label = len(expected_initiatives) == 1
        for initiative_name, counts in initiative_counts.items():
            expected = initiative_name in expected_initiatives
            predicted = initiative_name in predicted_initiatives
            if expected and predicted:
                counts["true_positive"] += 1
            elif predicted:
                counts["false_positive"] += 1
            elif expected:
                counts["false_negative"] += 1
            if expected:
                counts["support"] += 1
                if has_per_initiative_relevance_label:
                    counts["relevance_support"] += 1
                    if predicted_match_map.get(initiative_name, {}).get("relevance") == expected_relevance:
                        initiative_relevance_matches[initiative_name] += 1

        unexpected_initiatives = predicted_initiatives - expected_initiatives
        if (
            expected_detected != predicted_detected
            or expected_relevance != result["relevance"]
            or not expected_initiatives.issubset(predicted_initiatives)
            or (expected_initiatives and unexpected_initiatives)
        ):
            mismatches.append(
                {
                    "source": row.get("source", ""),
                    "title": row.get("title", ""),
                    "expected_relevance": expected_relevance,
                    "actual_relevance": result["relevance"],
                    "expected_initiatives": sorted(expected_initiatives),
                    "actual_initiatives": result["initiatives"],
                    "score": result["score"],
                }
            )

    detection = calculate_binary_metrics(
        detection_counts["true_positive"],
        detection_counts["false_positive"],
        detection_counts["false_negative"],
    )
    initiatives = {}
    for name, counts in initiative_counts.items():
        metrics = calculate_binary_metrics(
            counts["true_positive"],
            counts["false_positive"],
            counts["false_negative"],
        )
        metrics["support"] = counts["support"]
        metrics["relevance_support"] = counts["relevance_support"]
        metrics["relevance_accuracy"] = (
            initiative_relevance_matches[name] / counts["relevance_support"]
            if counts["relevance_support"]
            else None
        )
        initiatives[name] = metrics

    return {
        "row_count": len(rows),
        "detection": detection,
        "exact_relevance_accuracy": exact_relevance_matches / len(rows) if rows else 1.0,
        "initiatives": initiatives,
        "mismatches": mismatches,
    }
