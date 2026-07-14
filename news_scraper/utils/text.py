import re

from ..config import (
    AI_POLICY_GENERAL_KEYWORDS,
    AI_POLICY_HIGH_SCORE,
    AI_POLICY_INITIATIVES,
    AI_POLICY_KEYWORDS,
    AI_POLICY_NEGATIVE_KEYWORDS,
    AI_POLICY_NEGATIVE_PENALTY,
    AI_POLICY_POSSIBLE_SCORE,
)

NORMALIZED_AI_POLICY_KEYWORDS = None
DEPARTMENT_METADATA_PREFIX_RE = re.compile(
    r"^(?:版權[來來]自|版權所有|發佈單位|發布單位|提供機關|資料來源|資料來源單位|來源)[:：]\s*"
)
INTERNAL_DEPARTMENT_CODE_RE = re.compile(r"^\d+(?:\.\d+){3,}$")
URL_LIKE_RE = re.compile(r"^(?:https?|ftp):\s*/+\s*\S+|^www\.\S+", re.IGNORECASE)


def clean_text(text):
    if not text:
        return ""
    return re.sub(r"\s+", " ", str(text)).strip()


def normalize_title_for_dedupe(title):
    text = clean_text(title).replace("（", "(").replace("）", ")")
    return re.sub(r"\s+", "", text)


def normalize_keyword_match_text(text):
    text = clean_text(text).lower()
    text = text.replace("（", "(").replace("）", ")")
    return re.sub(r"\s+", "", text)


def get_normalized_ai_policy_keywords():
    global NORMALIZED_AI_POLICY_KEYWORDS
    if NORMALIZED_AI_POLICY_KEYWORDS is None:
        NORMALIZED_AI_POLICY_KEYWORDS = tuple(
            keyword
            for keyword in (
                normalize_keyword_match_text(raw_keyword)
                for raw_keyword in AI_POLICY_KEYWORDS
            )
            if keyword
        )
    return NORMALIZED_AI_POLICY_KEYWORDS


def keyword_matches_normalized_text(normalized_text, keyword):
    normalized_keyword = normalize_keyword_match_text(keyword)
    if not normalized_keyword:
        return False
    if re.fullmatch(r"[a-z0-9]{1,4}", normalized_keyword):
        return bool(
            re.search(
                r"(?<![a-z0-9]){}(?![a-z0-9])".format(re.escape(normalized_keyword)),
                normalized_text,
            )
        )
    return normalized_keyword in normalized_text


def find_matching_keywords(normalized_text, keywords):
    return [
        keyword
        for keyword in keywords
        if keyword_matches_normalized_text(normalized_text, keyword)
    ]


def build_empty_ai_policy_result(negative_keywords=None, reasons=None):
    return {
        "relevance": "",
        "score": 0,
        "initiatives": [],
        "lead_agencies": [],
        "matched_keywords": [],
        "negative_keywords": list(negative_keywords or []),
        "reasons": list(reasons or []),
        "initiative_matches": [],
    }


def classify_ai_policy_relevance(title, source="", summary=""):
    normalized_title = normalize_keyword_match_text(title)
    normalized_summary = normalize_keyword_match_text(summary)
    if not normalized_title and not normalized_summary:
        return build_empty_ai_policy_result()

    title_general_matches = find_matching_keywords(normalized_title, AI_POLICY_GENERAL_KEYWORDS)
    summary_general_matches = find_matching_keywords(normalized_summary, AI_POLICY_GENERAL_KEYWORDS)
    negative_matches = find_matching_keywords(normalized_title, AI_POLICY_NEGATIVE_KEYWORDS)
    matches = []
    for initiative in AI_POLICY_INITIATIVES:
        title_exact = find_matching_keywords(normalized_title, initiative.exact_phrases)
        title_strong = find_matching_keywords(normalized_title, initiative.strong_keywords)
        title_context = find_matching_keywords(normalized_title, initiative.context_keywords)
        summary_exact = find_matching_keywords(normalized_summary, initiative.exact_phrases)
        summary_strong = find_matching_keywords(normalized_summary, initiative.strong_keywords)
        summary_context = find_matching_keywords(normalized_summary, initiative.context_keywords)
        is_lead_source = source == initiative.lead_source

        candidate_scores = []
        candidate_reasons = []
        if title_exact:
            candidate_scores.append(100)
            candidate_reasons.append("標題命中完整建設名稱")
        if title_strong:
            candidate_scores.append(85)
            candidate_reasons.append("標題命中精準詞")
        if title_context and title_general_matches:
            candidate_scores.append(80 if is_lead_source else 65)
            candidate_reasons.append("標題同時命中 AI 語境與輔助詞")
        elif title_context and is_lead_source:
            candidate_scores.append(50)
            candidate_reasons.append("主政部會標題命中輔助詞")
        if summary_exact:
            candidate_scores.append(70)
            candidate_reasons.append("摘要命中完整建設名稱")
        if summary_strong:
            candidate_scores.append(60)
            candidate_reasons.append("摘要命中精準詞")
        if summary_context and (summary_general_matches or title_general_matches):
            candidate_scores.append(55 if is_lead_source else 45)
            candidate_reasons.append("摘要同時命中 AI 語境與輔助詞")
        elif summary_context and is_lead_source:
            candidate_scores.append(40)
            candidate_reasons.append("主政部會摘要命中輔助詞")

        if not candidate_scores:
            continue

        score = max(candidate_scores)
        if negative_matches:
            score = max(0, score - AI_POLICY_NEGATIVE_PENALTY)
            candidate_reasons.append("標題命中排除詞，分數下修")
        if score < AI_POLICY_POSSIBLE_SCORE:
            continue

        if score >= AI_POLICY_HIGH_SCORE:
            relevance = "高度相關"
        else:
            relevance = "可能相關"

        matches.append(
            {
                "name": initiative.name,
                "lead_agency": initiative.lead_agency,
                "relevance": relevance,
                "score": score,
                "keywords": (
                    title_exact
                    + title_strong
                    + title_context
                    + summary_exact
                    + summary_strong
                    + summary_context
                    + title_general_matches
                    + summary_general_matches
                ),
                "reasons": candidate_reasons,
            }
        )

    all_general_matches = list(dict.fromkeys(title_general_matches + summary_general_matches))
    if not matches and all_general_matches:
        score = AI_POLICY_POSSIBLE_SCORE
        reasons = ["僅命中一般 AI 詞，需人工判讀"]
        if negative_matches:
            score = max(0, score - AI_POLICY_NEGATIVE_PENALTY)
            reasons.append("標題命中排除詞，分數下修")
        if score < AI_POLICY_POSSIBLE_SCORE:
            return build_empty_ai_policy_result(negative_matches, reasons)
        return {
            "relevance": "可能相關",
            "score": score,
            "initiatives": ["待人工判讀"],
            "lead_agencies": ["待人工判讀"],
            "matched_keywords": all_general_matches,
            "negative_keywords": negative_matches,
            "reasons": reasons,
            "initiative_matches": [
                {
                    "name": "待人工判讀",
                    "lead_agency": "待人工判讀",
                    "relevance": "可能相關",
                    "score": score,
                    "matched_keywords": all_general_matches,
                    "reasons": reasons,
                }
            ],
        }
    if not matches:
        return build_empty_ai_policy_result(negative_matches)

    score = max(match["score"] for match in matches)
    relevance = "高度相關" if score >= AI_POLICY_HIGH_SCORE else "可能相關"
    initiative_matches = [
        {
            "name": match["name"],
            "lead_agency": match["lead_agency"],
            "relevance": match["relevance"],
            "score": match["score"],
            "matched_keywords": list(dict.fromkeys(match["keywords"])),
            "reasons": list(dict.fromkeys(match["reasons"])),
        }
        for match in matches
    ]
    return {
        "relevance": relevance,
        "score": score,
        "initiatives": list(dict.fromkeys(match["name"] for match in matches)),
        "lead_agencies": list(dict.fromkeys(match["lead_agency"] for match in matches)),
        "matched_keywords": list(
            dict.fromkeys(keyword for match in matches for keyword in match["keywords"])
        ),
        "negative_keywords": negative_matches,
        "reasons": list(
            dict.fromkeys(
                "{}：{}".format(match["name"], reason)
                for match in matches
                for reason in match["reasons"]
            )
        ),
        "initiative_matches": initiative_matches,
    }


def title_matches_ai_policy_keywords(title):
    return bool(classify_ai_policy_relevance(title)["relevance"])


def get_xml_local_name(tag):
    if not tag:
        return ""
    if "}" in tag:
        return tag.split("}", 1)[1]
    if ":" in tag:
        return tag.split(":", 1)[1]
    return tag


def xml_child_text_by_localnames(element, local_names):
    if element is None:
        return ""

    for child in list(element):
        if get_xml_local_name(child.tag) in local_names:
            return clean_text("".join(child.itertext()))
    return ""


def normalize_department_metadata_text(text):
    normalized = clean_text(text)
    while normalized:
        updated = DEPARTMENT_METADATA_PREFIX_RE.sub("", normalized).strip()
        if updated == normalized:
            break
        normalized = updated

    compact_normalized = re.sub(r"\s+", "", normalized)
    if INTERNAL_DEPARTMENT_CODE_RE.fullmatch(normalized) or URL_LIKE_RE.match(compact_normalized):
        return ""
    return normalized


def build_department_label(source, raw_department, aliases=None):
    aliases = {clean_text(alias) for alias in (aliases or set()) if clean_text(alias)}
    aliases.add(clean_text(source))

    department_text = normalize_department_metadata_text(raw_department)
    if not department_text:
        return source

    parts = [
        normalize_department_metadata_text(part)
        for part in re.split(r"\s*[>＞]\s*", department_text)
        if normalize_department_metadata_text(part)
    ]
    while parts and parts[0] in aliases:
        parts.pop(0)

    if parts:
        department_text = "／".join(parts)

    if not department_text or department_text in aliases:
        return source

    return "{}／{}".format(source, department_text)
