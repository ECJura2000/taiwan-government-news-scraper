use super::transport::TransportPolicy;
use super::ScraperError;
use reqwest::{
    header::{
        HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, ETAG,
        IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, PRAGMA, RANGE, RETRY_AFTER,
    },
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Mutex, Notify};

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    insecure_tls_client: Client,
    host_limits: Arc<Mutex<HashMap<String, Arc<HostGate>>>>,
    cache_dir: Option<PathBuf>,
    tls_fallback_hosts: Arc<Mutex<HashSet<String>>>,
}

#[derive(Serialize, Deserialize)]
struct CachedResponse {
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    body: String,
}

#[derive(Default)]
struct HostGate {
    active_limits: std::sync::Mutex<Vec<usize>>,
    notify: Notify,
}

struct HostPermit {
    gate: Arc<HostGate>,
    limit: usize,
}

impl Drop for HostPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.gate.active_limits.lock() {
            if let Some(index) = active.iter().position(|limit| *limit == self.limit) {
                active.swap_remove(index);
            }
        }
        self.gate.notify.notify_one();
    }
}

impl HttpClient {
    pub fn new() -> Result<Self, ScraperError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7"),
        );
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("HTTP client initialization failed: {error}"))
            })?;
        let insecure_tls_client = Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("TLS fallback client initialization failed: {error}"))
            })?;
        Ok(Self {
            client,
            insecure_tls_client,
            host_limits: Arc::new(Mutex::new(HashMap::new())),
            cache_dir: None,
            tls_fallback_hosts: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub fn with_cache_dir(mut self, directory: PathBuf) -> Self {
        self.cache_dir = Some(directory);
        self
    }

    pub async fn tls_fallback_hosts(&self) -> Vec<String> {
        let mut hosts = self
            .tls_fallback_hosts
            .lock()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        hosts.sort();
        hosts
    }

    pub async fn fetch_text(
        &self,
        url: &str,
        policy: &TransportPolicy,
    ) -> Result<String, ScraperError> {
        let mut last_error = None;
        for attempt in 0..policy.retry_attempts {
            match self.fetch_once(&self.client, url, policy).await {
                Ok(body) => return Ok(body),
                Err(error)
                    if policy.allows_tls_fallback_for(url)
                        && matches!(
                            error,
                            ScraperError::TlsCertificate(_) | ScraperError::RunnerNetwork(_)
                        ) =>
                {
                    let result = self
                        .fetch_once(&self.insecure_tls_client, url, policy)
                        .await;
                    if result.is_ok() {
                        if let Some(host) = url::Url::parse(url)
                            .ok()
                            .and_then(|parsed| parsed.host_str().map(str::to_owned))
                        {
                            self.tls_fallback_hosts.lock().await.insert(host);
                        }
                    }
                    return result;
                }
                Err(error) if error.retryable() && attempt + 1 < policy.retry_attempts => {
                    last_error = Some(error);
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ScraperError::Unknown("HTTP retry exhausted".into())))
    }

    pub async fn fetch_detail_text(
        &self,
        url: &str,
        policy: &TransportPolicy,
    ) -> Result<String, ScraperError> {
        self.fetch_text(url, policy).await
    }

    pub async fn fetch_range_prefix(
        &self,
        url: &str,
        policy: &TransportPolicy,
        end_byte: usize,
    ) -> Result<Vec<u8>, ScraperError> {
        for attempt in 0..policy.retry_attempts {
            match self.fetch_range_once(url, policy, end_byte).await {
                Ok(body) => return Ok(body),
                Err(error) if error.retryable() && attempt + 1 < policy.retry_attempts => {
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(ScraperError::Unknown(
            "range request retry exhausted".into(),
        ))
    }
    async fn fetch_range_once(
        &self,
        url: &str,
        policy: &TransportPolicy,
        end_byte: usize,
    ) -> Result<Vec<u8>, ScraperError> {
        let permit = self.acquire_host(url, policy.host_concurrency).await?;
        let response = self
            .client
            .get(url)
            .header(RANGE, format!("bytes=0-{end_byte}"))
            .header(ACCEPT_ENCODING, "identity")
            .timeout(policy.timeout)
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            if let Some(delay) = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(parse_retry_after)
            {
                drop(permit);
                drop(response);
                if delay > Duration::from_secs(60) {
                    return Err(ScraperError::AccessBlocked(format!(
                        "HTTP 429 Retry-After {}s exceeds run wait budget",
                        delay.as_secs()
                    )));
                }
                tokio::time::sleep(delay).await;
                return Err(ScraperError::SourceOutage(
                    "HTTP 429 Too Many Requests".into(),
                ));
            }
        }
        validate_status(status)?;
        read_response_bytes(response, policy.max_response_bytes).await
    }

    async fn fetch_once(
        &self,
        client: &Client,
        url: &str,
        policy: &TransportPolicy,
    ) -> Result<String, ScraperError> {
        let permit = self.acquire_host(url, policy.host_concurrency).await?;
        let cached = if policy.cache {
            self.read_cache(url)
                .filter(|entry| entry.body.len() <= policy.max_response_bytes)
        } else {
            None
        };
        let mut request = client.get(url).timeout(policy.timeout);
        if let Some(cached) = &cached {
            if let Some(etag) = &cached.etag {
                request = request.header(IF_NONE_MATCH, etag);
            } else if let Some(modified) = &cached.last_modified {
                request = request.header(IF_MODIFIED_SINCE, modified);
            }
        }
        let response = request.send().await.map_err(classify_request_error)?;
        let status = response.status();
        if status == StatusCode::NOT_MODIFIED {
            return cached.map(|entry| entry.body).ok_or_else(|| {
                ScraperError::SourceOutage("HTTP 304 without a cached body".into())
            });
        }
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let retry_after = if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers()
                .get(RETRY_AFTER)
                .and_then(parse_retry_after)
        } else {
            None
        };
        if let Some(delay) = retry_after {
            drop(permit);
            drop(response);
            if delay > Duration::from_secs(60) {
                return Err(ScraperError::AccessBlocked(format!(
                    "HTTP 429 Retry-After {}s exceeds run wait budget",
                    delay.as_secs()
                )));
            }
            tokio::time::sleep(delay).await;
            return Err(ScraperError::SourceOutage(
                "HTTP 429 Too Many Requests".into(),
            ));
        }
        validate_status(status)?;
        let bytes = read_response_bytes(response, policy.max_response_bytes).await?;
        let body = String::from_utf8_lossy(&bytes).into_owned();
        if policy.cache && (etag.is_some() || last_modified.is_some()) && body.len() <= 2_000_000 {
            self.write_cache(&CachedResponse {
                url: url.to_owned(),
                etag,
                last_modified,
                body: body.clone(),
            });
        }
        Ok(body)
    }

    async fn acquire_host(
        &self,
        url: &str,
        concurrency: usize,
    ) -> Result<HostPermit, ScraperError> {
        let concurrency = concurrency.max(1);
        let host = url::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .ok_or_else(|| ScraperError::Unknown(format!("invalid fetch URL: {url}")))?;
        let gate = {
            let mut limits = self.host_limits.lock().await;
            limits
                .entry(host)
                .or_insert_with(|| Arc::new(HostGate::default()))
                .clone()
        };
        loop {
            let notified = gate.notify.notified();
            {
                let mut active = gate.active_limits.lock().map_err(|error| {
                    ScraperError::Unknown(format!("host request limiter poisoned: {error}"))
                })?;
                let current_limit = active.iter().copied().min().unwrap_or(usize::MAX);
                if active.len() < concurrency && active.len() < current_limit {
                    active.push(concurrency);
                    return Ok(HostPermit {
                        gate: gate.clone(),
                        limit: concurrency,
                    });
                }
            }
            notified.await;
        }
    }

    fn read_cache(&self, url: &str) -> Option<CachedResponse> {
        let path = cache_path(self.cache_dir.as_deref()?, url);
        let bytes = std::fs::read(path).ok()?;
        let cached: CachedResponse = serde_json::from_slice(&bytes).ok()?;
        (cached.url == url).then_some(cached)
    }

    fn write_cache(&self, response: &CachedResponse) {
        let Some(directory) = &self.cache_dir else {
            return;
        };
        if std::fs::create_dir_all(directory).is_err() {
            return;
        }
        let path = cache_path(directory, &response.url);
        if let Ok(bytes) = serde_json::to_vec(response) {
            if let Ok(mut temporary) = tempfile::NamedTempFile::new_in(directory) {
                if temporary.write_all(&bytes).is_ok() {
                    let _ = temporary.persist(path);
                }
            }
        }
    }
}

async fn read_response_bytes(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, ScraperError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(ScraperError::AccessBlocked(format!(
            "response exceeds configured limit of {max_bytes} bytes"
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))?
    {
        if chunk.len() > max_bytes.saturating_sub(body.len()) {
            return Err(ScraperError::AccessBlocked(format!(
                "response exceeds configured limit of {max_bytes} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn cache_path(directory: &Path, url: &str) -> PathBuf {
    let hash = Sha256::digest(url.as_bytes());
    let name = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    directory.join(format!("{name}.json"))
}

fn retry_delay(attempt: usize) -> Duration {
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_millis() % 125);
    Duration::from_millis(300 * (1 << attempt.min(5)) + u64::from(jitter))
}

fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
    let text = value.to_str().ok()?;
    if let Ok(seconds) = text.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let deadline = chrono::DateTime::parse_from_rfc2822(text).ok()?;
    let seconds = (deadline.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds();
    Some(Duration::from_secs(seconds.max(0) as u64))
}

fn classify_request_error(error: reqwest::Error) -> ScraperError {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("certificate") {
        ScraperError::TlsCertificate(message)
    } else if error.is_timeout() || error.is_connect() {
        ScraperError::RunnerNetwork(message)
    } else if error.is_request() {
        ScraperError::SourceOutage(message)
    } else {
        ScraperError::Unknown(message)
    }
}

fn validate_status(status: StatusCode) -> Result<(), ScraperError> {
    if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
        return Err(ScraperError::AccessBlocked(format!("HTTP {status}")));
    }
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(ScraperError::SourceOutage(format!("HTTP {status}")));
    }
    if !status.is_success() {
        return Err(ScraperError::ParserRegression(format!("HTTP {status}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_can_be_constructed() {
        assert!(HttpClient::new().is_ok());
    }

    #[test]
    fn retry_after_seconds_and_cache_paths_are_bounded() {
        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("3")),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("900")),
            Some(Duration::from_secs(900))
        );
        assert_ne!(
            cache_path(Path::new("cache"), "https://example.test/a"),
            cache_path(Path::new("cache"), "https://example.test/b")
        );
    }

    #[tokio::test]
    async fn conditional_get_reuses_304_and_accepts_changed_etag_body() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut bytes = [0_u8; 4096];
                let count = socket.read(&mut bytes).unwrap();
                requests.push(String::from_utf8_lossy(&bytes[..count]).to_ascii_lowercase());
                let response = if index == 0 {
                    "HTTP/1.1 200 OK\r\nETag: \"version-1\"\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnews"
                } else if index == 1 {
                    "HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nETag: \"version-2\"\r\nContent-Length: 7\r\nConnection: close\r\n\r\nupdated"
                };
                socket.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        let directory = tempfile::tempdir().unwrap();
        let client = HttpClient::new()
            .unwrap()
            .with_cache_dir(directory.path().to_path_buf());
        let policy = TransportPolicy::list_default();
        assert_eq!(client.fetch_text(&url, &policy).await.unwrap(), "news");
        assert_eq!(client.fetch_text(&url, &policy).await.unwrap(), "news");
        assert_eq!(client.fetch_text(&url, &policy).await.unwrap(), "updated");
        let requests = server.join().unwrap();
        assert!(requests[1].contains("if-none-match: \"version-1\""));
    }

    #[tokio::test]
    async fn long_retry_after_stops_this_run_without_early_retry() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request);
            socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let client = HttpClient::new().unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            client.fetch_text(&url, &TransportPolicy::list_default()),
        )
        .await
        .unwrap();
        server.join().unwrap();
        assert!(matches!(result, Err(ScraperError::AccessBlocked(_))));
    }

    #[tokio::test]
    async fn configured_retry_recovers_a_transient_server_error() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (status, body) in [("503 Service Unavailable", ""), ("200 OK", "ready")] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request);
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let policy = TransportPolicy {
            retry_attempts: 2,
            ..TransportPolicy::list_default()
        };
        assert_eq!(
            HttpClient::new()
                .unwrap()
                .fetch_text(&url, &policy)
                .await
                .unwrap(),
            "ready"
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn short_retry_after_is_honored_then_retried() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for response in [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request);
                socket.write_all(response.as_bytes()).unwrap();
            }
        });
        let policy = TransportPolicy {
            retry_attempts: 2,
            ..TransportPolicy::list_default()
        };
        assert_eq!(
            HttpClient::new()
                .unwrap()
                .fetch_text(&url, &policy)
                .await
                .unwrap(),
            "ok"
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn configured_host_limit_serializes_requests() {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let server = {
            let active = active.clone();
            let peak = peak.clone();
            std::thread::spawn(move || {
                let mut handlers = Vec::new();
                for _ in 0..2 {
                    let (mut socket, _) = listener.accept().unwrap();
                    let active = active.clone();
                    let peak = peak.clone();
                    handlers.push(std::thread::spawn(move || {
                        let mut request = [0_u8; 1024];
                        let _ = socket.read(&mut request);
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(100));
                        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();
                        active.fetch_sub(1, Ordering::SeqCst);
                    }));
                }
                for handler in handlers {
                    handler.join().unwrap();
                }
            })
        };
        let client = HttpClient::new().unwrap();
        let policy = TransportPolicy {
            host_concurrency: 1,
            ..TransportPolicy::list_default()
        };
        let (first, second) = tokio::join!(
            client.fetch_text(&url, &policy),
            client.fetch_text(&url, &policy)
        );
        assert_eq!(first.unwrap(), "ok");
        assert_eq!(second.unwrap(), "ok");
        server.join().unwrap();
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stricter_policy_waits_for_existing_same_host_requests() {
        let client = HttpClient::new().unwrap();
        let url = "https://example.test/news";
        let first = client.acquire_host(url, 2).await.unwrap();
        let second = client.acquire_host(url, 2).await.unwrap();
        let strict = client.acquire_host(url, 1);
        tokio::pin!(strict);
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut strict)
            .await
            .is_err());
        drop(first);
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut strict)
            .await
            .is_err());
        drop(second);
        let strict_permit = tokio::time::timeout(Duration::from_secs(1), &mut strict)
            .await
            .unwrap()
            .unwrap();
        drop(strict_permit);
    }

    #[tokio::test]
    async fn configured_response_limit_rejects_instead_of_truncating() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/news", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request);
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                    )
                    .unwrap();
            }
        });
        let strict = TransportPolicy {
            max_response_bytes: 4,
            ..TransportPolicy::list_default()
        };
        assert!(matches!(
            HttpClient::new().unwrap().fetch_text(&url, &strict).await,
            Err(ScraperError::AccessBlocked(_))
        ));
        let exact = TransportPolicy {
            max_response_bytes: 5,
            ..TransportPolicy::list_default()
        };
        assert_eq!(
            HttpClient::new()
                .unwrap()
                .fetch_text(&url, &exact)
                .await
                .unwrap(),
            "hello"
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn streamed_response_without_content_length_obeys_limit() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/stream", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request);
            socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n").unwrap();
        });
        let policy = TransportPolicy {
            max_response_bytes: 4,
            ..TransportPolicy::list_default()
        };
        assert!(matches!(
            HttpClient::new().unwrap().fetch_text(&url, &policy).await,
            Err(ScraperError::AccessBlocked(_))
        ));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn configured_timeout_stops_a_slow_response() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/slow", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request);
            std::thread::sleep(Duration::from_millis(1500));
            let _ = socket.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nlate",
            );
        });
        let policy = TransportPolicy {
            timeout: Duration::from_secs(1),
            retry_attempts: 1,
            ..TransportPolicy::list_default()
        };
        assert!(matches!(
            HttpClient::new().unwrap().fetch_text(&url, &policy).await,
            Err(ScraperError::RunnerNetwork(_))
        ));
        server.join().unwrap();
    }
}
