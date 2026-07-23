from scripts.check_coverage import calculate_core_coverage, validate_coverage


def make_file_report(covered: int, statements: int) -> dict:
    return {
        "summary": {
            "covered_lines": covered,
            "num_statements": statements,
            "percent_covered": covered * 100.0 / statements,
        }
    }


def test_coverage_gate_checks_core_and_each_critical_file():
    files = {
        "news_scraper/application.py": make_file_report(7, 10),
        "news_scraper/gui.py": make_file_report(0, 100),
        "news_scraper/scrapers/ministry/example.py": make_file_report(0, 100),
        "news_scraper/http/client.py": make_file_report(8, 10),
        "news_scraper/main.py": make_file_report(8, 10),
        "news_scraper/monitoring.py": make_file_report(8, 10),
        "news_scraper/scrapers/ministry/veterans/vghtpe.py": make_file_report(8, 10),
    }

    assert calculate_core_coverage(files) == 77.5
    assert validate_coverage({"files": files}) == []


def test_coverage_gate_reports_low_critical_file():
    files = {
        "news_scraper/application.py": make_file_report(8, 10),
        "news_scraper/http/client.py": make_file_report(7, 10),
        "news_scraper/main.py": make_file_report(8, 10),
        "news_scraper/monitoring.py": make_file_report(8, 10),
        "news_scraper/scrapers/ministry/veterans/vghtpe.py": make_file_report(8, 10),
    }

    failures = validate_coverage({"files": files})

    assert any("news_scraper/http/client.py" in failure for failure in failures)
