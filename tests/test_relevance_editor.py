from news_scraper.relevance import ExclusionRule, KeywordRule, RelevanceProfile, TopicRule
from news_scraper.relevance_editor import RelevanceProfileEditor


class FakeTree:
    def __init__(self, selected=""):
        self.selected = selected

    def selection(self):
        return (self.selected,) if self.selected else ()

    def selection_set(self, item_id):
        self.selected = item_id


class FakeMessageBox:
    def askyesno(self, *_args, **_kwargs):
        return True

    def showinfo(self, *_args, **_kwargs):
        return None


def build_editor():
    topic = TopicRule(
        id="default:topic:one",
        name="資安",
        core_keywords=[
            KeywordRule(
                id="default:keyword:one",
                text="零信任",
                origin="default",
            )
        ],
        origin="default",
    )
    profile = RelevanceProfile(
        name="編輯測試",
        topics=[topic],
        exclusions=[
            ExclusionRule(
                id="default:exclusion:one",
                text="課程",
                topic_id=topic.id,
                origin="default",
            )
        ],
    )
    editor = object.__new__(RelevanceProfileEditor)
    editor.profile = profile
    editor.topic_tree = FakeTree(topic.id)
    editor.keyword_tree = FakeTree(topic.core_keywords[0].id)
    editor.exclusion_tree = FakeTree(profile.exclusions[0].id)
    editor.deleted_stack = []
    editor.messagebox = FakeMessageBox()
    editor.window = None
    editor._refresh_all = lambda: None
    return editor


def test_topic_actions_duplicate_sort_toggle_delete_and_undo():
    editor = build_editor()
    original_topic = editor.profile.topics[0]

    editor._duplicate_topic()

    duplicated = editor.profile.topics[1]
    assert duplicated.id.startswith("custom:topic:")
    assert duplicated.core_keywords[0].id.startswith("custom:keyword:")
    assert duplicated.name == "資安（副本）"
    duplicated_exclusions = [
        rule
        for rule in editor.profile.exclusions
        if rule.topic_id == duplicated.id
    ]
    assert len(duplicated_exclusions) == 1
    assert duplicated_exclusions[0].id.startswith("custom:exclusion:")

    editor._toggle_topic()
    assert duplicated.enabled is False
    editor._move_topic(-1)
    assert editor.profile.topics[0] is duplicated

    editor._delete_topic()
    assert duplicated not in editor.profile.topics
    assert not [
        rule for rule in editor.profile.exclusions if rule.topic_id == duplicated.id
    ]
    editor._undo_delete()
    assert editor.profile.topics[0] is duplicated
    assert [
        rule for rule in editor.profile.exclusions if rule.topic_id == duplicated.id
    ]
    assert original_topic in editor.profile.topics


def test_keyword_and_exclusion_actions_toggle_delete_tombstone_and_undo():
    editor = build_editor()
    keyword = editor.profile.topics[0].core_keywords[0]
    exclusion = editor.profile.exclusions[0]

    editor._toggle_keyword()
    assert keyword.enabled is False
    editor._delete_keyword()
    assert editor.profile.topics[0].core_keywords == []
    assert keyword.id in editor.profile.deleted_default_ids
    editor._undo_delete()
    assert editor.profile.topics[0].core_keywords == [keyword]
    assert keyword.id not in editor.profile.deleted_default_ids

    editor._toggle_exclusion()
    assert exclusion.enabled is False
    editor._delete_exclusion()
    assert exclusion not in editor.profile.exclusions
    assert exclusion.id in editor.profile.deleted_default_ids
    editor._undo_delete()
    assert editor.profile.exclusions == [exclusion]
    assert exclusion.id not in editor.profile.deleted_default_ids
