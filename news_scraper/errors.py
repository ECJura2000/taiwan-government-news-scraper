class NewsScraperError(Exception):
    retryable = False


class DownloadError(NewsScraperError):
    retryable = True


class ParseError(NewsScraperError):
    pass


class ValidationError(NewsScraperError):
    pass


class StorageError(NewsScraperError):
    pass


def is_retryable_error(error: BaseException) -> bool:
    return bool(getattr(error, "retryable", False))
