import re

from ..config import AI_POLICY_KEYWORDS

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
    from ..config import AI_POLICY_INITIATIVES
    from ..relevance import classify_relevance

    result = classify_relevance(title, source=source, summary=summary)
    lead_agency_map = {
        initiative.name: initiative.lead_agency
        for initiative in AI_POLICY_INITIATIVES
    }

    def legacy_reason(reason):
        return (
            reason.replace("完整主題名稱", "完整建設名稱")
            .replace("核心詞", "精準詞")
            .replace("脈絡詞與輔助詞", "AI 語境與輔助詞")
            .replace("優先關聯機關", "主政部會")
            .replace("僅命中全域脈絡詞", "僅命中一般 AI 詞")
        )

    if result["relevance"] == "未納入":
        return build_empty_ai_policy_result(
            result["excluded_keywords"],
            [legacy_reason(reason) for reason in result["reasons"]],
        )

    initiative_matches = [
        {
            "name": match["name"],
            "lead_agency": lead_agency_map.get(match["name"], "待人工判讀"),
            "relevance": match["relevance"],
            "score": match["score"],
            "matched_keywords": match["matched_keywords"],
            "reasons": [legacy_reason(reason) for reason in match["reasons"]],
        }
        for match in result["topic_matches"]
    ]
    return {
        "relevance": result["relevance"],
        "score": result["score"],
        "initiatives": result["topics"],
        "lead_agencies": [
            lead_agency_map.get(name, "待人工判讀")
            for name in result["topics"]
        ],
        "matched_keywords": result["matched_keywords"],
        "negative_keywords": result["excluded_keywords"],
        "reasons": [legacy_reason(reason) for reason in result["reasons"]],
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
