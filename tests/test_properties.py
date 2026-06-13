from hypothesis import given, strategies as st

from news_scraper.quality import normalize_url
from news_scraper.utils.text import normalize_title_for_dedupe


@given(st.text())
def test_title_normalization_is_idempotent(value):
    normalized = normalize_title_for_dedupe(value)
    assert normalize_title_for_dedupe(normalized) == normalized


@given(st.text())
def test_url_normalization_never_crashes(value):
    assert isinstance(normalize_url(value), str)
