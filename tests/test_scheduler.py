from news_scraper.scheduler import prioritize_sources


def test_priority_queue_starts_recent_failures_before_static_difficulty():
    reports = [
        {
            "source_attempts": [
                {"source": "財政部", "status": "failed", "elapsed_seconds": 2},
                {"source": "國土管理署", "status": "success", "elapsed_seconds": 20},
            ]
        }
    ]

    jobs = prioritize_sources(["國土管理署", "財政部"], reports)

    assert [job.source for job in jobs] == ["財政部", "國土管理署"]
    assert jobs[0].reason["failure_rate"] == 1.0


def test_priority_queue_uses_longer_history_duration_before_static_difficulty():
    reports = [
        {
            "source_attempts": [
                {"source": "行政院", "status": "success", "elapsed_seconds": 30},
                {"source": "公路局", "status": "success", "elapsed_seconds": 2},
            ]
        }
    ]

    jobs = prioritize_sources(["行政院", "公路局"], reports)

    assert [job.source for job in jobs] == ["行政院", "公路局"]


def test_source_history_treats_retry_success_as_one_successful_run():
    reports = [
        {
            "source_attempts": [
                {"source": "財政部", "status": "failed", "elapsed_seconds": 2},
                {"source": "財政部", "status": "success", "elapsed_seconds": 3},
            ]
        }
    ]

    job = prioritize_sources(["財政部"], reports)[0]

    assert job.reason["attempts"] == 1
    assert job.reason["failure_rate"] == 0
    assert job.reason["average_elapsed_seconds"] == 5
