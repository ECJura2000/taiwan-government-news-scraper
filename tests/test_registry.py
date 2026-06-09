from news_scraper.config import (
    AFFILIATED_GROUPS,
    AFFILIATED_SOURCE_PATHS,
    ORDERED_SOURCE_NAMES,
    SCRAPE_DIFFICULTY_ORDER,
    SOURCE_ORDER,
)
from news_scraper.scrapers.registry import SCRAPER_REGISTRY
from news_scraper.policy import load_policy


def test_registry_contains_core_sources():
    for source in ["行政院", "監察院", "司法院", "財政部", "法務部", "內政部", "勞動部", "交通部", "故宮", "榮總"]:
        assert source in SCRAPER_REGISTRY


def test_registry_has_full_source_count():
    assert len(SCRAPER_REGISTRY) == len(SOURCE_ORDER) == 71


def test_registry_matches_source_order_keys():
    assert set(SCRAPER_REGISTRY) == set(SOURCE_ORDER)


def test_scrape_difficulty_order_only_references_known_sources():
    assert set(SCRAPE_DIFFICULTY_ORDER) == set(SOURCE_ORDER)


def test_registry_iteration_follows_source_order():
    assert list(SCRAPER_REGISTRY) == ORDERED_SOURCE_NAMES


def test_affiliated_source_paths_reference_known_sources():
    for source_name, path in AFFILIATED_SOURCE_PATHS.items():
        assert source_name in SOURCE_ORDER
        assert path[0] in SOURCE_ORDER
        assert path[-1] == source_name


def test_affiliated_groups_are_derived_from_source_paths():
    for source_name, path in AFFILIATED_SOURCE_PATHS.items():
        parent_source = path[0]
        assert parent_source in AFFILIATED_GROUPS
        assert source_name in AFFILIATED_GROUPS[parent_source]["members"]
        assert AFFILIATED_GROUPS[parent_source]["priority"][source_name] == 0
        assert AFFILIATED_GROUPS[parent_source]["priority"][parent_source] == 1


def test_zero_item_policies_only_reference_known_sources():
    assert set(load_policy()["zero_items"]["sources"]) <= set(SOURCE_ORDER)
