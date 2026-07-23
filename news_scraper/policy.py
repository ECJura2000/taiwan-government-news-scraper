import os
from functools import lru_cache
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib

DEFAULT_POLICY_PATH = Path(__file__).with_name("policy.toml")


@lru_cache(maxsize=1)
def load_policy():
    policy_path = Path(os.environ.get("NEWS_SCRAPER_POLICY_FILE", DEFAULT_POLICY_PATH))
    with policy_path.open("rb") as policy_file:
        return tomllib.load(policy_file)


def get_zero_item_alert_runs(source):
    zero_item_policy = load_policy()["zero_items"]
    return int(zero_item_policy["sources"].get(source, zero_item_policy["default_runs"]))


def get_quality_alert_thresholds():
    return dict(load_policy()["quality"])


def get_summary_coverage_policy():
    return dict(load_policy().get("summary_coverage", {}))
