from news_scraper.runtime import collect_required_dependencies


def test_collect_required_dependencies_includes_optional_dependencies_for_all_sources():
    dependencies = collect_required_dependencies()

    assert dependencies["feedparser"] == "feedparser"


def test_collect_required_dependencies_skips_unneeded_optional_dependencies_for_selected_sources():
    dependencies = collect_required_dependencies(selected_sources=["財政部"])

    assert "feedparser" not in dependencies


def test_collect_required_dependencies_includes_optional_dependencies_for_matching_source():
    dependencies = collect_required_dependencies(selected_sources=["農業部"])

    assert dependencies["feedparser"] == "feedparser"


def test_collect_required_dependencies_includes_selenium_for_nlma_source():
    dependencies = collect_required_dependencies(selected_sources=["國土管理署"])

    assert dependencies["selenium"] == "selenium"


def test_collect_required_dependencies_skips_optional_dependencies_when_listing_sources():
    dependencies = collect_required_dependencies(list_sources_only=True)

    assert "feedparser" not in dependencies
    assert "selenium" not in dependencies
