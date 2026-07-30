from news_scraper.errors import (
    DownloadError,
    ParseError,
    ParserContractError,
    StorageError,
    ValidationError,
    is_retryable_error,
)


def test_only_download_errors_are_retryable():
    assert is_retryable_error(DownloadError("temporary"))
    assert not is_retryable_error(ParseError("bad payload"))
    assert not is_retryable_error(ValidationError("bad schema"))
    assert not is_retryable_error(StorageError("disk full"))


def test_parser_contract_error_exposes_response_evidence():
    error = ParserContractError(
        "missing list",
        url="https://example.test/news",
        content_type="text/html",
        response_bytes=321,
        selector=".news",
    )

    assert isinstance(error, ParseError)
    assert error.failure_class == "parser_regression"
    assert error.response_bytes == 321
