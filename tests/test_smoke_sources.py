import importlib.util
from pathlib import Path


def load_smoke_module():
    path = Path(__file__).resolve().parents[1] / "scripts" / "smoke_sources.py"
    spec = importlib.util.spec_from_file_location("smoke_sources", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_select_sources_keeps_high_risk_and_rotates_all_sources():
    smoke = load_smoke_module()
    all_sources = ["行政院", "監察院", "司法院", "財政部", "榮總", "工程會"]

    first = smoke.select_sources(all_sources, batch_count=2, batch_index=0)
    second = smoke.select_sources(all_sources, batch_count=2, batch_index=1)

    assert {"榮總", "司法院", "財政部"} <= set(first)
    assert {"榮總", "司法院", "財政部"} <= set(second)
    assert set(first) | set(second) == set(all_sources)


def test_parse_json_summary_uses_last_machine_readable_line():
    smoke = load_smoke_module()

    summary = smoke.parse_json_summary('log line\n{"status":"success","news_count":2}\n')

    assert summary == {"status": "success", "news_count": 2}
