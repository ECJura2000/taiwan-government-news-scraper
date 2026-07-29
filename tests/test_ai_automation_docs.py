from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_readme_exposes_source_downloads_and_ai_guide():
    readme = (PROJECT_ROOT / "README.md").read_text(encoding="utf-8")

    assert "archive/refs/tags/v1.5.0.zip" in readme
    assert "archive/refs/heads/main.zip" in readme
    assert "docs/AI_AUTOMATION.md" in readme
    assert "AGENTS.md" in readme


def test_ai_agent_contract_uses_machine_readable_headless_run():
    contract = (PROJECT_ROOT / "AGENTS.md").read_text(encoding="utf-8")
    guide = (PROJECT_ROOT / "docs" / "AI_AUTOMATION.md").read_text(
        encoding="utf-8"
    )

    for text in (contract, guide):
        assert "--headless --json-summary" in text
        assert "failed_sources" in text
        assert "quality.alert_reasons" in text
        assert "relevance_policy.ruleset_hash" in text

    assert ".venv/bin/python" in contract
    assert r".venv\Scripts\python.exe" in contract


def test_release_and_portable_guides_keep_ai_source_entrypoint():
    workflow = (
        PROJECT_ROOT / ".github" / "workflows" / "build-release.yml"
    ).read_text(encoding="utf-8")
    portable_guide = (
        PROJECT_ROOT / "docs" / "PORTABLE_README.txt"
    ).read_text(encoding="utf-8")

    assert (
        "archive/refs/tags/${RELEASE_TAG}.zip"
        in workflow
    )
    assert "blob/${RELEASE_TAG}/docs/AI_AUTOMATION.md" in workflow
    assert (
        "https://github.com/ECJura2000/taiwan-government-news-scraper/"
        "blob/main/docs/AI_AUTOMATION.md"
        in portable_guide
    )
