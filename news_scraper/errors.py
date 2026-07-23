class NewsScraperError(Exception):
    retryable = False


class DownloadError(NewsScraperError):
    retryable = True


class CurlRequestError(DownloadError):
    def __init__(
        self,
        message: str,
        *,
        url: str,
        exit_code: int | None = None,
        http_status: int | None = None,
        error_category: str = "connection",
    ) -> None:
        super().__init__(message)
        self.url = url
        self.exit_code = exit_code
        self.http_status = http_status
        self.error_category = error_category


class ParseError(NewsScraperError):
    pass


class ValidationError(NewsScraperError):
    pass


class StorageError(NewsScraperError):
    pass


def is_retryable_error(error: BaseException) -> bool:
    return bool(getattr(error, "retryable", False))
