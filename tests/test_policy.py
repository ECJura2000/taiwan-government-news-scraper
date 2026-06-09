from news_scraper import policy


def test_policy_can_be_loaded_from_external_toml(tmp_path, monkeypatch):
    policy_path = tmp_path / "policy.toml"
    policy_path.write_text(
        """
[zero_items]
default_runs = 7
[zero_items.sources]
"測試來源" = 3
[quality]
invalid_count = 2
duplicate_count = 6
duplicate_ratio = 0.3
excluded_non_news_count = 4
excluded_non_news_ratio = 0.4
""".strip(),
        encoding="utf-8",
    )
    monkeypatch.setenv("NEWS_SCRAPER_POLICY_FILE", str(policy_path))
    policy.load_policy.cache_clear()

    assert policy.get_zero_item_alert_runs("測試來源") == 3
    assert policy.get_zero_item_alert_runs("其他來源") == 7
    assert policy.get_quality_alert_thresholds()["invalid_count"] == 2

    policy.load_policy.cache_clear()
