import copy
import hashlib
import json
import re
import shutil
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Iterable, TypedDict

from .io_utils import atomic_write_text

PROFILE_SCHEMA_VERSION = 1
RELEVANCE_HIGH_SCORE = 80
RELEVANCE_POSSIBLE_SCORE = 40
RELEVANCE_EXCLUSION_PENALTY = 50
VALID_MATCH_FIELDS = frozenset({"title", "summary"})
VALID_ORIGINS = frozenset({"default", "custom", "legacy"})


def normalize_rule_text(text: str) -> str:
    normalized = re.sub(r"\s+", "", str(text or "").strip().lower())
    return normalized.replace("（", "(").replace("）", ")")


def _keyword_matches(normalized_text: str, keyword: str) -> bool:
    normalized_keyword = normalize_rule_text(keyword)
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


def _stable_default_id(kind: str, *parts: str) -> str:
    payload = "|".join((kind, *(normalize_rule_text(part) for part in parts)))
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]
    return "default:{}:{}".format(kind, digest)


def new_custom_id(kind: str) -> str:
    return "custom:{}:{}".format(kind, uuid.uuid4().hex)


@dataclass
class KeywordRule:
    id: str
    text: str
    enabled: bool = True
    origin: str = "custom"

    @classmethod
    def from_dict(cls, data: dict) -> "KeywordRule":
        return cls(
            id=str(data.get("id", "")),
            text=str(data.get("text", "")),
            enabled=bool(data.get("enabled", True)),
            origin=str(data.get("origin", "custom")),
        )


@dataclass
class ExclusionRule:
    id: str
    text: str
    topic_id: str = ""
    match_fields: list[str] = field(default_factory=lambda: ["title"])
    enabled: bool = True
    origin: str = "custom"

    @classmethod
    def from_dict(cls, data: dict) -> "ExclusionRule":
        return cls(
            id=str(data.get("id", "")),
            text=str(data.get("text", "")),
            topic_id=str(data.get("topic_id", "")),
            match_fields=[str(value) for value in data.get("match_fields", ["title"])],
            enabled=bool(data.get("enabled", True)),
            origin=str(data.get("origin", "custom")),
        )


@dataclass
class TopicRule:
    id: str
    name: str
    description: str = ""
    display_color: str = "#FFFF00"
    enabled: bool = True
    match_name: bool = True
    priority_sources: list[str] = field(default_factory=list)
    core_keywords: list[KeywordRule] = field(default_factory=list)
    supporting_keywords: list[KeywordRule] = field(default_factory=list)
    context_keywords: list[KeywordRule] = field(default_factory=list)
    origin: str = "custom"

    @classmethod
    def from_dict(cls, data: dict) -> "TopicRule":
        return cls(
            id=str(data.get("id", "")),
            name=str(data.get("name", "")),
            description=str(data.get("description", "")),
            display_color=str(data.get("display_color", "#FFFF00")),
            enabled=bool(data.get("enabled", True)),
            match_name=bool(data.get("match_name", True)),
            priority_sources=[str(value) for value in data.get("priority_sources", [])],
            core_keywords=[
                KeywordRule.from_dict(value) for value in data.get("core_keywords", [])
            ],
            supporting_keywords=[
                KeywordRule.from_dict(value) for value in data.get("supporting_keywords", [])
            ],
            context_keywords=[
                KeywordRule.from_dict(value) for value in data.get("context_keywords", [])
            ],
            origin=str(data.get("origin", "custom")),
        )


@dataclass
class RelevanceProfile:
    schema_version: int = PROFILE_SCHEMA_VERSION
    name: str = "AI 新十大建設預設範本"
    template_version: str = ""
    include_unassigned_context_matches: bool = True
    topics: list[TopicRule] = field(default_factory=list)
    global_context_keywords: list[KeywordRule] = field(default_factory=list)
    exclusions: list[ExclusionRule] = field(default_factory=list)
    deleted_default_ids: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "RelevanceProfile":
        return cls(
            schema_version=int(data.get("schema_version", 0)),
            name=str(data.get("name", "")),
            template_version=str(data.get("template_version", "")),
            include_unassigned_context_matches=bool(
                data.get("include_unassigned_context_matches", True)
            ),
            topics=[TopicRule.from_dict(value) for value in data.get("topics", [])],
            global_context_keywords=[
                KeywordRule.from_dict(value)
                for value in data.get("global_context_keywords", [])
            ],
            exclusions=[
                ExclusionRule.from_dict(value) for value in data.get("exclusions", [])
            ],
            deleted_default_ids=[
                str(value) for value in data.get("deleted_default_ids", [])
            ],
        )

    def to_dict(self) -> dict:
        return asdict(self)

    def clone(self) -> "RelevanceProfile":
        return copy.deepcopy(self)


@dataclass(frozen=True)
class LoadedRelevanceProfile:
    profile: RelevanceProfile
    source_path: Path | None
    source_label: str


class _TopicMatch(TypedDict):
    id: str
    name: str
    priority_sources: list[str]
    relevance: str
    score: int
    keywords: list[str]
    excluded_keywords: list[str]
    reasons: list[str]


class ProfileDifference(TypedDict):
    added: list[str]
    modified: list[str]
    removed: list[str]
    added_count: int
    modified_count: int
    removed_count: int


def build_default_relevance_profile() -> RelevanceProfile:
    from .config import (
        AI_POLICY_GENERAL_KEYWORDS,
        AI_POLICY_INITIATIVES,
        AI_POLICY_NEGATIVE_KEYWORDS,
        AI_POLICY_RULESET_VERSION,
    )

    topics = []
    for initiative in AI_POLICY_INITIATIVES:
        topic_id = _stable_default_id("topic", initiative.name)
        topics.append(
            TopicRule(
                id=topic_id,
                name=initiative.name,
                description="由既有 AI 新十大建設規則轉換的預設主題。",
                priority_sources=[initiative.lead_source],
                core_keywords=[
                    KeywordRule(
                        id=_stable_default_id("core", initiative.name, keyword),
                        text=keyword,
                        origin="default",
                    )
                    for keyword in initiative.strong_keywords
                ],
                supporting_keywords=[
                    KeywordRule(
                        id=_stable_default_id("supporting", initiative.name, keyword),
                        text=keyword,
                        origin="default",
                    )
                    for keyword in initiative.context_keywords
                ],
                origin="default",
            )
        )

    return RelevanceProfile(
        name="AI 新十大建設預設範本",
        template_version=AI_POLICY_RULESET_VERSION,
        include_unassigned_context_matches=True,
        topics=topics,
        global_context_keywords=[
            KeywordRule(
                id=_stable_default_id("global-context", keyword),
                text=keyword,
                origin="default",
            )
            for keyword in AI_POLICY_GENERAL_KEYWORDS
        ],
        exclusions=[
            ExclusionRule(
                id=_stable_default_id("exclusion", keyword),
                text=keyword,
                match_fields=["title"],
                origin="default",
            )
            for keyword in AI_POLICY_NEGATIVE_KEYWORDS
        ],
    )


def _iter_topic_keywords(topic: TopicRule) -> Iterable[tuple[str, KeywordRule]]:
    for category, rules in (
        ("core", topic.core_keywords),
        ("supporting", topic.supporting_keywords),
        ("context", topic.context_keywords),
    ):
        for rule in rules:
            yield category, rule


def _iter_all_ids(profile: RelevanceProfile) -> Iterable[tuple[str, str]]:
    for topic in profile.topics:
        yield topic.id, "主題 {}".format(topic.name)
        for category, keyword_rule in _iter_topic_keywords(topic):
            yield keyword_rule.id, "{} 的 {} 關鍵字 {}".format(
                topic.name,
                category,
                keyword_rule.text,
            )
    for keyword_rule in profile.global_context_keywords:
        yield keyword_rule.id, "全域脈絡詞 {}".format(keyword_rule.text)
    for exclusion_rule in profile.exclusions:
        yield exclusion_rule.id, "排除詞 {}".format(exclusion_rule.text)


def validate_relevance_profile(
    profile: RelevanceProfile,
    *,
    available_sources: Iterable[str] | None = None,
) -> RelevanceProfile:
    errors = []
    if profile.schema_version != PROFILE_SCHEMA_VERSION:
        errors.append(
            "不支援的設定格式版本：{}（目前支援 {}）".format(
                profile.schema_version,
                PROFILE_SCHEMA_VERSION,
            )
        )
    if not profile.name.strip():
        errors.append("設定名稱不可空白")

    seen_ids: dict[str, str] = {}
    for item_id, label in _iter_all_ids(profile):
        if not item_id:
            errors.append("{} 缺少 ID".format(label))
        elif item_id in seen_ids:
            errors.append("ID 重複：{} 與 {}".format(seen_ids[item_id], label))
        else:
            seen_ids[item_id] = label

    enabled_topics = [topic for topic in profile.topics if topic.enabled]
    if not enabled_topics:
        errors.append("至少需要一個啟用的主題")

    source_set = set(available_sources or ())
    topic_ids = {topic.id for topic in profile.topics}
    seen_topic_names: set[str] = set()
    for topic in profile.topics:
        normalized_topic_name = normalize_rule_text(topic.name)
        if not normalized_topic_name:
            errors.append("主題名稱不可空白")
        elif normalized_topic_name in seen_topic_names:
            errors.append("主題名稱重複：{}".format(topic.name))
        else:
            seen_topic_names.add(normalized_topic_name)
        if not re.fullmatch(r"#[0-9A-Fa-f]{6}", topic.display_color):
            errors.append("{} 的顯示顏色必須是 #RRGGBB".format(topic.name or "未命名主題"))
        if source_set:
            unknown = sorted(set(topic.priority_sources) - source_set)
            if unknown:
                errors.append(
                    "{} 包含未知機關：{}".format(topic.name, "、".join(unknown))
                )

        seen_topic_keywords: dict[str, str] = {}
        for category, keyword_rule in _iter_topic_keywords(topic):
            normalized = normalize_rule_text(keyword_rule.text)
            if not normalized:
                errors.append("{} 包含空白的 {} 關鍵字".format(topic.name, category))
            elif keyword_rule.enabled and normalized in seen_topic_keywords:
                errors.append(
                    "{} 的啟用關鍵字重複：{}".format(topic.name, keyword_rule.text)
                )
            elif keyword_rule.enabled:
                seen_topic_keywords[normalized] = category
            if keyword_rule.origin not in VALID_ORIGINS:
                errors.append(
                    "{} 的來源類型無效：{}".format(
                        keyword_rule.text,
                        keyword_rule.origin,
                    )
                )

    seen_global_context: set[str] = set()
    for keyword_rule in profile.global_context_keywords:
        normalized = normalize_rule_text(keyword_rule.text)
        if not normalized:
            errors.append("全域脈絡詞不可空白")
        elif keyword_rule.enabled and normalized in seen_global_context:
            errors.append("啟用的全域脈絡詞重複：{}".format(keyword_rule.text))
        elif keyword_rule.enabled:
            seen_global_context.add(normalized)
        if keyword_rule.origin not in VALID_ORIGINS:
            errors.append(
                "{} 的來源類型無效：{}".format(
                    keyword_rule.text,
                    keyword_rule.origin,
                )
            )

    active_exclusions: dict[tuple[str, str], ExclusionRule] = {}
    for exclusion_rule in profile.exclusions:
        normalized = normalize_rule_text(exclusion_rule.text)
        if not normalized:
            errors.append("排除詞不可空白")
        if exclusion_rule.topic_id and exclusion_rule.topic_id not in topic_ids:
            errors.append("{} 指向不存在的主題".format(exclusion_rule.text))
        fields = set(exclusion_rule.match_fields)
        if not fields or not fields <= VALID_MATCH_FIELDS:
            errors.append("{} 的比對欄位無效".format(exclusion_rule.text))
        if exclusion_rule.origin not in VALID_ORIGINS:
            errors.append(
                "{} 的來源類型無效：{}".format(
                    exclusion_rule.text,
                    exclusion_rule.origin,
                )
            )
        key = (exclusion_rule.topic_id, normalized)
        if exclusion_rule.enabled and key in active_exclusions:
            errors.append("啟用的排除詞重複：{}".format(exclusion_rule.text))
        elif exclusion_rule.enabled:
            active_exclusions[key] = exclusion_rule

    global_exclusion_texts = {
        normalize_rule_text(rule.text)
        for rule in profile.exclusions
        if rule.enabled and not rule.topic_id
    }
    for keyword_rule in profile.global_context_keywords:
        if (
            keyword_rule.enabled
            and normalize_rule_text(keyword_rule.text) in global_exclusion_texts
        ):
            errors.append(
                "關鍵字同時設為全域脈絡詞與排除詞：{}".format(
                    keyword_rule.text
                )
            )
    for topic in profile.topics:
        if not topic.enabled:
            continue
        topic_exclusion_texts = global_exclusion_texts | {
            normalize_rule_text(rule.text)
            for rule in profile.exclusions
            if rule.enabled and rule.topic_id == topic.id
        }
        for _category, keyword_rule in _iter_topic_keywords(topic):
            if (
                keyword_rule.enabled
                and normalize_rule_text(keyword_rule.text) in topic_exclusion_texts
            ):
                errors.append(
                    "{} 的關鍵字同時設為納入與排除：{}".format(
                        topic.name,
                        keyword_rule.text,
                    )
                )
        if (
            topic.match_name
            and normalize_rule_text(topic.name) in topic_exclusion_texts
        ):
            errors.append(
                "{} 的主題名稱同時設為納入與排除".format(topic.name)
            )

    if errors:
        raise ValueError("關聯性設定無效：{}".format("；".join(dict.fromkeys(errors))))
    return profile


def _merge_keyword_list(
    current: list[KeywordRule],
    defaults: list[KeywordRule],
    tombstones: set[str],
) -> None:
    current_ids = {rule.id for rule in current}
    current.extend(
        copy.deepcopy(rule)
        for rule in defaults
        if rule.id not in current_ids and rule.id not in tombstones
    )


def merge_new_default_rules(
    profile: RelevanceProfile,
    defaults: RelevanceProfile | None = None,
) -> tuple[RelevanceProfile, bool]:
    merged = profile.clone()
    defaults = defaults or build_default_relevance_profile()
    before = json.dumps(merged.to_dict(), ensure_ascii=False, sort_keys=True)
    tombstones = set(merged.deleted_default_ids)

    topic_map = {topic.id: topic for topic in merged.topics}
    for default_topic in defaults.topics:
        if default_topic.id in tombstones:
            continue
        current_topic = topic_map.get(default_topic.id)
        if current_topic is None:
            merged.topics.append(copy.deepcopy(default_topic))
            continue
        _merge_keyword_list(
            current_topic.core_keywords,
            default_topic.core_keywords,
            tombstones,
        )
        _merge_keyword_list(
            current_topic.supporting_keywords,
            default_topic.supporting_keywords,
            tombstones,
        )
        _merge_keyword_list(
            current_topic.context_keywords,
            default_topic.context_keywords,
            tombstones,
        )

    _merge_keyword_list(
        merged.global_context_keywords,
        defaults.global_context_keywords,
        tombstones,
    )
    exclusion_ids = {rule.id for rule in merged.exclusions}
    merged.exclusions.extend(
        copy.deepcopy(rule)
        for rule in defaults.exclusions
        if rule.id not in exclusion_ids and rule.id not in tombstones
    )
    merged.template_version = defaults.template_version
    after = json.dumps(merged.to_dict(), ensure_ascii=False, sort_keys=True)
    return merged, before != after


def get_default_profile_path() -> Path:
    from .paths import prepare_workspace

    return prepare_workspace().program_data / "relevance-profile.json"


def save_relevance_profile(
    path: str | Path,
    profile: RelevanceProfile,
    *,
    available_sources: Iterable[str] | None = None,
    backup: bool = False,
) -> Path:
    destination = Path(path)
    validate_relevance_profile(profile, available_sources=available_sources)
    if backup and destination.exists():
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        shutil.copy2(destination, destination.with_name("{}-{}.json".format(destination.stem, timestamp)))
    return atomic_write_text(
        destination,
        json.dumps(profile.to_dict(), ensure_ascii=False, indent=2) + "\n",
    )


def load_relevance_profile(
    path: str | Path | None = None,
    *,
    use_saved_profile: bool = True,
    default_path: str | Path | None = None,
    available_sources: Iterable[str] | None = None,
    merge_defaults: bool = True,
) -> LoadedRelevanceProfile:
    if not use_saved_profile:
        profile = build_default_relevance_profile()
        validate_relevance_profile(profile, available_sources=available_sources)
        return LoadedRelevanceProfile(profile, None, "內建預設範本")

    explicit_path = Path(path).expanduser() if path else None
    source_path = (
        explicit_path
        or (Path(default_path).expanduser() if default_path else None)
        or get_default_profile_path()
    )
    if not source_path.exists():
        if explicit_path is not None:
            raise ValueError("找不到關聯性設定檔：{}".format(source_path))
        profile = build_default_relevance_profile()
        validate_relevance_profile(profile, available_sources=available_sources)
        return LoadedRelevanceProfile(profile, None, "內建預設範本")

    try:
        payload = json.loads(source_path.read_text(encoding="utf-8"))
        profile = RelevanceProfile.from_dict(payload)
    except (OSError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise ValueError("無法讀取關聯性設定檔 {}：{}".format(source_path, exc)) from exc

    changed = False
    if merge_defaults and profile.template_version:
        profile, changed = merge_new_default_rules(profile)
    validate_relevance_profile(profile, available_sources=available_sources)
    if changed and explicit_path is None:
        save_relevance_profile(
            source_path,
            profile,
            available_sources=available_sources,
        )
    return LoadedRelevanceProfile(profile, source_path.resolve(), str(source_path.resolve()))


def _active_texts(rules: Iterable[KeywordRule]) -> list[str]:
    return [rule.text for rule in rules if rule.enabled]


def _find_matches(normalized_text: str, rules: Iterable[KeywordRule]) -> list[str]:
    return [
        rule.text
        for rule in rules
        if rule.enabled and _keyword_matches(normalized_text, rule.text)
    ]


def _matching_exclusions(
    normalized_title: str,
    normalized_summary: str,
    rules: Iterable[ExclusionRule],
) -> list[str]:
    matches: list[str] = []
    for rule in rules:
        if not rule.enabled:
            continue
        matched = (
            "title" in rule.match_fields
            and _keyword_matches(normalized_title, rule.text)
        ) or (
            "summary" in rule.match_fields
            and _keyword_matches(normalized_summary, rule.text)
        )
        if matched:
            matches.append(rule.text)
    return list(dict.fromkeys(matches))


def build_empty_relevance_result(
    excluded_keywords: Iterable[str] | None = None,
    reasons: Iterable[str] | None = None,
) -> dict:
    return {
        "relevance": "",
        "score": 0,
        "topics": [],
        "priority_sources": [],
        "matched_keywords": [],
        "excluded_keywords": list(excluded_keywords or []),
        "reasons": list(reasons or []),
        "topic_matches": [],
    }


def _aggregate_topic_results(
    matches: list[_TopicMatch],
    *,
    relevance: str,
    score: int,
) -> dict:
    return {
        "relevance": relevance,
        "score": score,
        "topics": list(dict.fromkeys(match["name"] for match in matches)),
        "priority_sources": list(
            dict.fromkeys(
                priority_source
                for match in matches
                for priority_source in match["priority_sources"]
            )
        ),
        "matched_keywords": list(
            dict.fromkeys(
                keyword for match in matches for keyword in match["keywords"]
            )
        ),
        "excluded_keywords": list(
            dict.fromkeys(
                keyword
                for match in matches
                for keyword in match["excluded_keywords"]
            )
        ),
        "reasons": list(
            dict.fromkeys(
                "{}：{}".format(match["name"], reason)
                for match in matches
                for reason in match["reasons"]
            )
        ),
        "topic_matches": [
            {
                "id": match["id"],
                "name": match["name"],
                "priority_sources": match["priority_sources"],
                "relevance": match["relevance"],
                "score": match["score"],
                "matched_keywords": match["keywords"],
                "excluded_keywords": match["excluded_keywords"],
                "reasons": match["reasons"],
            }
            for match in matches
        ],
    }


def classify_relevance(
    title: str,
    *,
    source: str = "",
    summary: str = "",
    profile: RelevanceProfile | None = None,
) -> dict:
    profile = profile or build_default_relevance_profile()
    normalized_title = normalize_rule_text(title)
    normalized_summary = normalize_rule_text(summary)
    if not normalized_title and not normalized_summary:
        return build_empty_relevance_result()

    title_global_context = _find_matches(
        normalized_title,
        profile.global_context_keywords,
    )
    summary_global_context = _find_matches(
        normalized_summary,
        profile.global_context_keywords,
    )
    global_exclusions = [
        rule for rule in profile.exclusions if not rule.topic_id
    ]
    global_exclusion_matches = _matching_exclusions(
        normalized_title,
        normalized_summary,
        global_exclusions,
    )

    matches: list[_TopicMatch] = []
    candidate_matches: list[_TopicMatch] = []
    all_exclusion_matches = list(global_exclusion_matches)
    for topic in profile.topics:
        if not topic.enabled:
            continue

        title_name = (
            [topic.name]
            if topic.match_name and _keyword_matches(normalized_title, topic.name)
            else []
        )
        summary_name = (
            [topic.name]
            if topic.match_name and _keyword_matches(normalized_summary, topic.name)
            else []
        )
        title_core = _find_matches(normalized_title, topic.core_keywords)
        summary_core = _find_matches(normalized_summary, topic.core_keywords)
        title_supporting = _find_matches(
            normalized_title,
            topic.supporting_keywords,
        )
        summary_supporting = _find_matches(
            normalized_summary,
            topic.supporting_keywords,
        )
        title_topic_context = _find_matches(
            normalized_title,
            topic.context_keywords,
        )
        summary_topic_context = _find_matches(
            normalized_summary,
            topic.context_keywords,
        )
        title_context = list(
            dict.fromkeys(title_global_context + title_topic_context)
        )
        summary_context = list(
            dict.fromkeys(summary_global_context + summary_topic_context)
        )
        is_priority_source = source in topic.priority_sources

        candidate_scores = []
        candidate_reasons = []
        if title_name:
            candidate_scores.append(100)
            candidate_reasons.append("標題命中完整主題名稱")
        if title_core:
            candidate_scores.append(85)
            candidate_reasons.append("標題命中核心詞")
        if title_supporting and title_context:
            candidate_scores.append(80 if is_priority_source else 65)
            candidate_reasons.append("標題同時命中脈絡詞與輔助詞")
        elif title_supporting and is_priority_source:
            candidate_scores.append(50)
            candidate_reasons.append("優先關聯機關標題命中輔助詞")
        if summary_name:
            candidate_scores.append(70)
            candidate_reasons.append("摘要命中完整主題名稱")
        if summary_core:
            candidate_scores.append(60)
            candidate_reasons.append("摘要命中核心詞")
        if summary_supporting and (summary_context or title_context):
            candidate_scores.append(55 if is_priority_source else 45)
            candidate_reasons.append("摘要同時命中脈絡詞與輔助詞")
        elif summary_supporting and is_priority_source:
            candidate_scores.append(40)
            candidate_reasons.append("優先關聯機關摘要命中輔助詞")

        if not candidate_scores:
            continue

        topic_exclusions = [
            rule
            for rule in profile.exclusions
            if not rule.topic_id or rule.topic_id == topic.id
        ]
        excluded = _matching_exclusions(
            normalized_title,
            normalized_summary,
            topic_exclusions,
        )
        all_exclusion_matches.extend(excluded)
        score = max(candidate_scores)
        if excluded:
            score = max(0, score - RELEVANCE_EXCLUSION_PENALTY)
            candidate_reasons.append("命中排除詞，分數下修")
        relevance = (
            "高度相關"
            if score >= RELEVANCE_HIGH_SCORE
            else "可能相關"
            if score >= RELEVANCE_POSSIBLE_SCORE
            else "未納入"
        )
        candidate_match: _TopicMatch = {
            "id": topic.id,
            "name": topic.name,
            "priority_sources": list(topic.priority_sources),
            "relevance": relevance,
            "score": score,
            "keywords": list(
                dict.fromkeys(
                    title_name
                    + title_core
                    + title_supporting
                    + title_context
                    + summary_name
                    + summary_core
                    + summary_supporting
                    + summary_context
                )
            ),
            "excluded_keywords": excluded,
            "reasons": list(dict.fromkeys(candidate_reasons)),
        }
        candidate_matches.append(candidate_match)
        if score >= RELEVANCE_POSSIBLE_SCORE:
            matches.append(candidate_match)

    all_context_matches = list(
        dict.fromkeys(title_global_context + summary_global_context)
    )
    if candidate_matches and not matches:
        return _aggregate_topic_results(
            candidate_matches,
            relevance="未納入",
            score=max(match["score"] for match in candidate_matches),
        )
    if (
        not matches
        and profile.include_unassigned_context_matches
        and all_context_matches
    ):
        score = RELEVANCE_POSSIBLE_SCORE
        reasons = ["僅命中全域脈絡詞，需人工判讀"]
        if global_exclusion_matches:
            score = max(0, score - RELEVANCE_EXCLUSION_PENALTY)
            reasons.append("命中排除詞，分數下修")
        if score < RELEVANCE_POSSIBLE_SCORE:
            return {
                "relevance": "未納入",
                "score": score,
                "topics": ["待人工判讀"],
                "priority_sources": [],
                "matched_keywords": all_context_matches,
                "excluded_keywords": global_exclusion_matches,
                "reasons": reasons,
                "topic_matches": [
                    {
                        "id": "",
                        "name": "待人工判讀",
                        "priority_sources": [],
                        "relevance": "未納入",
                        "score": score,
                        "matched_keywords": all_context_matches,
                        "excluded_keywords": global_exclusion_matches,
                        "reasons": reasons,
                    }
                ],
            }
        return {
            "relevance": "可能相關",
            "score": score,
            "topics": ["待人工判讀"],
            "priority_sources": [],
            "matched_keywords": all_context_matches,
            "excluded_keywords": global_exclusion_matches,
            "reasons": reasons,
            "topic_matches": [
                {
                    "id": "",
                    "name": "待人工判讀",
                    "priority_sources": [],
                    "relevance": "可能相關",
                    "score": score,
                    "matched_keywords": all_context_matches,
                    "excluded_keywords": global_exclusion_matches,
                    "reasons": reasons,
                }
            ],
        }
    if not matches:
        exclusions = list(dict.fromkeys(all_exclusion_matches))
        if exclusions:
            return {
                **build_empty_relevance_result(
                    exclusions,
                    ["命中排除詞，但未命中納入規則"],
                ),
                "relevance": "未納入",
            }
        return build_empty_relevance_result()

    score = max(match["score"] for match in matches)
    return _aggregate_topic_results(
        candidate_matches,
        relevance=(
            "高度相關" if score >= RELEVANCE_HIGH_SCORE else "可能相關"
        ),
        score=score,
    )


def get_effective_relevance_hash(profile: RelevanceProfile) -> str:
    payload = {
        "schema_version": profile.schema_version,
        "include_unassigned_context_matches": profile.include_unassigned_context_matches,
        "topics": [
            {
                "id": topic.id,
                "name": topic.name,
                "enabled": topic.enabled,
                "match_name": topic.match_name,
                "priority_sources": topic.priority_sources,
                "core_keywords": [
                    asdict(rule) for rule in topic.core_keywords
                ],
                "supporting_keywords": [
                    asdict(rule) for rule in topic.supporting_keywords
                ],
                "context_keywords": [
                    asdict(rule) for rule in topic.context_keywords
                ],
            }
            for topic in profile.topics
        ],
        "global_context_keywords": [
            asdict(rule) for rule in profile.global_context_keywords
        ],
        "exclusions": [asdict(rule) for rule in profile.exclusions],
    }
    encoded = json.dumps(
        payload,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()[:16]


def get_relevance_profile_summary(
    profile: RelevanceProfile,
    *,
    source_label: str = "內建預設範本",
) -> dict:
    topic_keyword_rules = [
        rule
        for topic in profile.topics
        for _category, rule in _iter_topic_keywords(topic)
    ]
    all_keyword_rules = topic_keyword_rules + profile.global_context_keywords
    return {
        "schema_version": profile.schema_version,
        "name": profile.name,
        "template_version": profile.template_version,
        "ruleset_hash": get_effective_relevance_hash(profile),
        "source": source_label,
        "topic_count": len(profile.topics),
        "enabled_topic_count": sum(topic.enabled for topic in profile.topics),
        "disabled_topic_count": sum(not topic.enabled for topic in profile.topics),
        "keyword_count": len(all_keyword_rules),
        "enabled_keyword_count": sum(rule.enabled for rule in all_keyword_rules),
        "disabled_keyword_count": sum(not rule.enabled for rule in all_keyword_rules),
        "exclusion_count": len(profile.exclusions),
        "enabled_exclusion_count": sum(rule.enabled for rule in profile.exclusions),
        "disabled_exclusion_count": sum(
            not rule.enabled for rule in profile.exclusions
        ),
        "custom_item_count": sum(
            item_id.startswith("custom:")
            for item_id, _label in _iter_all_ids(profile)
        ),
        "deleted_default_count": len(profile.deleted_default_ids),
    }


def _profile_comparison_items(profile: RelevanceProfile) -> dict[str, tuple[str, dict]]:
    items: dict[str, tuple[str, dict]] = {
        "__profile__": (
            "設定屬性",
            {
                "schema_version": profile.schema_version,
                "name": profile.name,
                "include_unassigned_context_matches": (
                    profile.include_unassigned_context_matches
                ),
                "deleted_default_ids": sorted(profile.deleted_default_ids),
            },
        )
    }
    for topic in profile.topics:
        items[topic.id] = (
            "主題「{}」".format(topic.name),
            {
                "name": topic.name,
                "description": topic.description,
                "display_color": topic.display_color,
                "enabled": topic.enabled,
                "match_name": topic.match_name,
                "priority_sources": topic.priority_sources,
            },
        )
        for category, rules in (
            ("核心詞", topic.core_keywords),
            ("輔助詞", topic.supporting_keywords),
            ("脈絡詞", topic.context_keywords),
        ):
            for keyword_rule in rules:
                items[keyword_rule.id] = (
                    "{}「{}」的「{}」".format(
                        category,
                        keyword_rule.text,
                        topic.name,
                    ),
                    asdict(keyword_rule),
                )
    for context_rule in profile.global_context_keywords:
        items[context_rule.id] = (
            "全域脈絡詞「{}」".format(context_rule.text),
            asdict(context_rule),
        )
    for exclusion_rule in profile.exclusions:
        scope = next(
            (
                topic.name
                for topic in profile.topics
                if topic.id == exclusion_rule.topic_id
            ),
            "全域",
        )
        items[exclusion_rule.id] = (
            "{}排除詞「{}」".format(scope, exclusion_rule.text),
            asdict(exclusion_rule),
        )
    return items


def compare_relevance_profiles(
    current: RelevanceProfile,
    incoming: RelevanceProfile,
) -> ProfileDifference:
    current_items = _profile_comparison_items(current)
    incoming_items = _profile_comparison_items(incoming)
    added = [
        incoming_items[item_id][0]
        for item_id in incoming_items
        if item_id not in current_items
    ]
    removed = [
        current_items[item_id][0]
        for item_id in current_items
        if item_id not in incoming_items
    ]
    modified = [
        incoming_items[item_id][0]
        for item_id in incoming_items
        if item_id in current_items
        and incoming_items[item_id][1] != current_items[item_id][1]
    ]
    return {
        "added": added,
        "modified": modified,
        "removed": removed,
        "added_count": len(added),
        "modified_count": len(modified),
        "removed_count": len(removed),
    }


def delete_default_aware(profile: RelevanceProfile, item_id: str) -> None:
    if item_id.startswith("default:") and item_id not in profile.deleted_default_ids:
        profile.deleted_default_ids.append(item_id)
