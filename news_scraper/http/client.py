import json
import logging
import subprocess
import threading
import warnings
from typing import Any, Mapping
from urllib.parse import urljoin, urlparse

import requests
from requests.adapters import HTTPAdapter
from requests.exceptions import RequestException, SSLError, TooManyRedirects
from urllib3.exceptions import InsecureRequestWarning
from urllib3.util.retry import Retry

from ..errors import ParseError
from ..config import (
    HEADERS,
    REQUEST_TIMEOUT,
    RETRY_BACKOFF_FACTOR,
    RETRY_TOTAL,
    SSL_FALLBACK_HOSTS,
)

logger = logging.getLogger(__name__)

THREAD_LOCAL = threading.local()


def set_retry_timeout_extra_seconds(seconds: int) -> None:
    from ..monitoring import get_current_run_context

    context = get_current_run_context()
    if context is not None:
        context.retry_timeout_extra_seconds = max(0, int(seconds))


def get_effective_timeout(timeout: int | float | None) -> int | float | None:
    from ..monitoring import get_current_run_context

    if timeout is None:
        return None
    context = get_current_run_context()
    extra_seconds = context.retry_timeout_extra_seconds if context is not None else 0
    return timeout + extra_seconds


def get_thread_session(verify: bool = True) -> requests.Session:
    attr_name = "session_verify_true" if verify else "session_verify_false"
    session = getattr(THREAD_LOCAL, attr_name, None)
    if session is None:
        session = requests.Session()
        session.headers.update(HEADERS)
        session.verify = verify

        retry = Retry(
            total=RETRY_TOTAL,
            connect=RETRY_TOTAL,
            read=RETRY_TOTAL,
            status=RETRY_TOTAL,
            backoff_factor=RETRY_BACKOFF_FACTOR,
            status_forcelist=[429, 500, 502, 503, 504],
            allowed_methods=frozenset(["GET", "POST"]),
            raise_on_status=False,
            respect_retry_after_header=False,
        )
        adapter = HTTPAdapter(max_retries=retry, pool_connections=20, pool_maxsize=20)
        session.mount("http://", adapter)
        session.mount("https://", adapter)
        setattr(THREAD_LOCAL, attr_name, session)
    return session


def should_fallback_ssl(url: str) -> bool:
    return urlparse(url).netloc.lower() in SSL_FALLBACK_HOSTS


def is_insecure_ssl_allowed(url: str) -> bool:
    host = urlparse(url).netloc.lower()
    return host in SSL_FALLBACK_HOSTS


def should_skip_ssl_verify(url: str) -> bool:
    del url
    return False


def remember_ssl_verify_failure(url: str) -> None:
    del url


def record_insecure_ssl_use(url: str) -> None:
    from ..monitoring import get_current_run_context

    host = urlparse(url).netloc.lower()
    context = get_current_run_context()
    if context is not None:
        context.record_insecure_ssl_use(host)


def apply_response_encoding(response: requests.Response) -> None:
    content_type = response.headers.get("Content-Type", "").lower()
    if "charset=" not in content_type:
        response.encoding = response.apparent_encoding


def create_session(verify: bool = True) -> requests.Session:
    return get_thread_session(verify=verify)


def merge_headers(session: requests.Session, extra_headers: Mapping[str, str] | None = None) -> dict[str, str] | None:
    if not extra_headers:
        return None
    request_headers = {str(key): str(value) for key, value in session.headers.items()}
    request_headers.update(extra_headers)
    return request_headers


def send_request(
    session: requests.Session,
    method: str,
    url: str,
    timeout: int | float | None,
    data: Any = None,
    extra_headers: Mapping[str, str] | None = None,
    allow_redirects: bool = True,
) -> requests.Response:
    return session.request(
        method.upper(),
        url,
        data=data,
        timeout=timeout,
        headers=merge_headers(session, extra_headers),
        allow_redirects=allow_redirects,
    )


def fetch_response(
    url: str,
    method: str = "GET",
    data: Any = None,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> requests.Response:
    timeout = get_effective_timeout(timeout)
    verify_ssl = not should_skip_ssl_verify(url)
    session = create_session(verify=verify_ssl)
    if not verify_ssl:
        record_insecure_ssl_use(url)

    response = send_request(session, method, url, timeout, data=data, extra_headers=extra_headers)

    response.raise_for_status()
    apply_response_encoding(response)
    return response


def fetch_html(
    url: str,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> str:
    try:
        return fetch_response(url, method="GET", timeout=timeout, extra_headers=extra_headers).text
    except SSLError as ssl_error:
        try:
            logger.warning("Requests SSL 驗證失敗，先改用安全 curl：https://%s", urlparse(url).netloc)
            return fetch_html_by_curl_with_headers(
                url,
                timeout=timeout,
                extra_headers=extra_headers,
                insecure=False,
            )
        except (OSError, RequestException, subprocess.SubprocessError) as curl_error:
            if not is_insecure_ssl_allowed(url):
                raise ssl_error from curl_error
            remember_ssl_verify_failure(url)
            logger.warning("安全 curl 亦失敗，白名單來源最後改用 verify=False：%s", url)
            return fetch_html_plain_insecure(url, timeout=timeout, extra_headers=extra_headers)


def fetch_html_plain_insecure(
    url: str,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> str:
    timeout = get_effective_timeout(timeout)
    session = create_session(verify=False)
    current_url = url
    max_redirects = 10

    for _ in range(max_redirects + 1):
        if not is_insecure_ssl_allowed(current_url):
            raise SSLError("不允許對非白名單主機停用 SSL 驗證：{}".format(current_url))
        record_insecure_ssl_use(current_url)
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", InsecureRequestWarning)
            response = send_request(
                session,
                "GET",
                current_url,
                timeout,
                extra_headers=extra_headers,
                allow_redirects=False,
            )

        if response.status_code not in {301, 302, 303, 307, 308}:
            response.raise_for_status()
            apply_response_encoding(response)
            return response.text

        location = response.headers.get("Location", "").strip()
        if not location:
            response.raise_for_status()
            raise RequestException("重新導向回應缺少 Location：{}".format(current_url))
        current_url = urljoin(current_url, location)

    raise TooManyRedirects("不安全 SSL 抓取超過 {} 次重新導向：{}".format(max_redirects, url))


def fetch_json_data(
    url: str,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> Any:
    text = fetch_html(url, timeout=timeout, extra_headers=extra_headers)
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        raise ParseError("JSON 解析失敗：{} ({})".format(exc, url)) from exc


def fetch_html_by_curl(url: str, timeout: int | float | None = REQUEST_TIMEOUT) -> str:
    timeout = get_effective_timeout(timeout)
    if timeout is None:
        timeout = REQUEST_TIMEOUT
    command = [
        "curl",
        "--http1.1",
        "--connect-timeout",
        str(timeout),
        "--max-time",
        str(timeout),
        "-A",
        HEADERS["User-Agent"],
        "-H",
        "Accept: {}".format(HEADERS["Accept"]),
        "-H",
        "Accept-Language: {}".format(HEADERS["Accept-Language"]),
        url,
    ]
    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=timeout + 2,
        check=False,
    )
    if completed.returncode != 0:
        raise RequestException("curl 抓取失敗：{} ({})".format(completed.stderr.strip(), url))
    return completed.stdout


def fetch_html_by_curl_with_headers(
    url: str,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
    insecure: bool = False,
) -> str:
    timeout = get_effective_timeout(timeout)
    if timeout is None:
        timeout = REQUEST_TIMEOUT
    if insecure:
        if not is_insecure_ssl_allowed(url):
            raise SSLError("不允許對非白名單主機停用 SSL 驗證：{}".format(url))
        record_insecure_ssl_use(url)
    command = [
        "curl",
        "-L",
        "--http1.1",
        "--connect-timeout",
        str(timeout),
        "--max-time",
        str(timeout),
        "-sS",
        "-A",
        HEADERS["User-Agent"],
    ]
    if insecure:
        command.extend(["-L", "--max-redirs", "0"])
        command.append("-k")
    else:
        command.append("-L")

    headers = dict(HEADERS)
    if extra_headers:
        headers.update(extra_headers)
    for key, value in headers.items():
        command.extend(["-H", "{}: {}".format(key, value)])
    command.append(url)

    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=timeout + 2,
        check=False,
    )
    if completed.returncode != 0:
        raise RequestException("curl 抓取失敗：{} ({})".format(completed.stderr.strip(), url))
    return completed.stdout


def fetch_html_resilient(
    url: str,
    timeout: int | float | None = REQUEST_TIMEOUT,
    extra_headers: Mapping[str, str] | None = None,
) -> str:
    errors = []
    fetchers = [
        lambda: fetch_html(url, timeout=timeout, extra_headers=extra_headers),
        lambda: fetch_html_by_curl_with_headers(url, timeout=timeout, extra_headers=extra_headers),
    ]
    if is_insecure_ssl_allowed(url):
        fetchers.append(lambda: fetch_html_plain_insecure(url, timeout=timeout, extra_headers=extra_headers))

    for fetcher in fetchers:
        try:
            return fetcher()
        except (RequestException, OSError, subprocess.SubprocessError, ParseError) as exc:
            errors.append(exc)
    raise errors[-1]
