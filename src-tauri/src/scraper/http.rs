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
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const WDA_TIMEOUT: Duration = Duration::from_secs(25);
const NICS_PRIMARY_TIMEOUT: Duration = Duration::from_secs(8);
const DETAIL_FAST_FAIL_TIMEOUT: Duration = Duration::from_secs(8);
const RECENT_JSON_PREFIX_END: usize = 262_143;

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    wda_client: Client,
    nics_primary_client: Client,
    detail_fast_fail_client: Client,
    mnd_tls_fallback_client: Client,
    host_limits: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
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
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("HTTP client initialization failed: {error}"))
            })?;
        let mnd_tls_fallback_client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("MND TLS fallback client initialization failed: {error}"))
            })?;
        let wda_client = Client::builder()
            .timeout(WDA_TIMEOUT)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("WDA fast-fail client initialization failed: {error}"))
            })?;
        let nics_primary_client = Client::builder()
            .timeout(NICS_PRIMARY_TIMEOUT)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("NICS primary client initialization failed: {error}"))
            })?;
        let detail_fast_fail_client = Client::builder()
            .timeout(DETAIL_FAST_FAIL_TIMEOUT)
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            )
            .default_headers(headers.clone())
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("Detail fast-fail client initialization failed: {error}"))
            })?;
        Ok(Self {
            client,
            wda_client,
            nics_primary_client,
            detail_fast_fail_client,
            mnd_tls_fallback_client,
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

    pub async fn fetch_text(&self, url: &str) -> Result<String, ScraperError> {
        let mut last_error = None;
        let attempts = if is_wda_fast_fail_host(url) || is_nics_primary_host(url) {
            1
        } else {
            3
        };
        let client = if is_wda_fast_fail_host(url) {
            &self.wda_client
        } else if is_nics_primary_host(url) {
            &self.nics_primary_client
        } else {
            &self.client
        };
        for attempt in 0..attempts {
            match self.fetch_once(client, url, true).await {
                Ok(body) => return Ok(body),
                Err(error)
                    if is_mnd_tls_fallback_host(url)
                        && matches!(
                            error,
                            ScraperError::TlsCertificate(_) | ScraperError::RunnerNetwork(_)
                        ) =>
                {
                    let result = self
                        .fetch_once(&self.mnd_tls_fallback_client, url, true)
                        .await;
                    if result.is_ok() {
                        self.tls_fallback_hosts
                            .lock()
                            .await
                            .insert("www.mnd.gov.tw".into());
                    }
                    return result;
                }
                Err(error) if error.retryable() && attempt + 1 < attempts => {
                    last_error = Some(error);
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ScraperError::Unknown("HTTP retry exhausted".into())))
    }

    pub async fn fetch_detail_text(&self, url: &str) -> Result<String, ScraperError> {
        if is_detail_fast_fail_host(url) {
            self.fetch_once(&self.detail_fast_fail_client, url, false)
                .await
        } else {
            for attempt in 0..3 {
                match self.fetch_once(&self.client, url, false).await {
                    Ok(body) => return Ok(body),
                    Err(error) if error.retryable() && attempt < 2 => {
                        tokio::time::sleep(retry_delay(attempt)).await;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(ScraperError::Unknown("detail retry exhausted".into()))
        }
    }

    pub async fn fetch_recent_json_array(&self, url: &str) -> Result<String, ScraperError> {
        for attempt in 0..3 {
            match self.fetch_recent_json_once(url).await {
                Ok(body) => return Ok(body),
                Err(error) if error.retryable() && attempt < 2 => {
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(ScraperError::Unknown("recent JSON retry exhausted".into()))
    }

    async fn fetch_recent_json_once(&self, url: &str) -> Result<String, ScraperError> {
        let permit = self.acquire_host(url).await?;
        let response = self
            .client
            .get(url)
            .header(RANGE, format!("bytes=0-{RECENT_JSON_PREFIX_END}"))
            .header(ACCEPT_ENCODING, "identity")
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
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))?;
        complete_json_array_prefix(&bytes)
    }

    async fn fetch_once(
        &self,
        client: &Client,
        url: &str,
        cacheable: bool,
    ) -> Result<String, ScraperError> {
        let permit = self.acquire_host(url).await?;
        let cached = if cacheable {
            self.read_cache(url)
        } else {
            None
        };
        let mut request = client.get(url);
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
        let body = response
            .text()
            .await
            .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))?;
        if cacheable && (etag.is_some() || last_modified.is_some()) && body.len() <= 2_000_000 {
            self.write_cache(&CachedResponse {
                url: url.to_owned(),
                etag,
                last_modified,
                body: body.clone(),
            });
        }
        Ok(body)
    }

    async fn acquire_host(&self, url: &str) -> Result<OwnedSemaphorePermit, ScraperError> {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .ok_or_else(|| ScraperError::Unknown(format!("invalid fetch URL: {url}")))?;
        let limit = {
            let mut limits = self.host_limits.lock().await;
            limits
                .entry(host)
                .or_insert_with(|| Arc::new(Semaphore::new(2)))
                .clone()
        };
        limit
            .acquire_owned()
            .await
            .map_err(|error| ScraperError::Unknown(format!("host request limiter closed: {error}")))
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

fn complete_json_array_prefix(bytes: &[u8]) -> Result<String, ScraperError> {
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    let mut last_complete = None;
    let mut complete_array = false;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    last_complete = Some(index + 1);
                }
            }
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    complete_array = true;
                    last_complete = Some(index + 1);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(end) = last_complete else {
        return Err(ScraperError::ParserRegression(
            "recent JSON prefix did not contain a complete item".into(),
        ));
    };
    let mut completed = bytes[..end].to_vec();
    if !complete_array {
        completed.push(b']');
    }
    String::from_utf8(completed).map_err(|error| {
        ScraperError::ParserRegression(format!("recent JSON prefix is not UTF-8: {error}"))
    })
}

fn is_mnd_tls_fallback_host(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .as_deref()
        == Some("www.mnd.gov.tw")
}

fn is_wda_fast_fail_host(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .as_deref()
        == Some("www.wda.gov.tw")
}

fn is_nics_primary_host(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .as_deref()
        == Some("www.nics.nat.gov.tw")
}

fn is_detail_fast_fail_host(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            matches!(
                host.as_str(),
                "www.moj.gov.tw" | "www.nps.gov.tw" | "www.moa.gov.tw"
            )
        })
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
        assert_eq!(client.fetch_text(&url).await.unwrap(), "news");
        assert_eq!(client.fetch_text(&url).await.unwrap(), "news");
        assert_eq!(client.fetch_text(&url).await.unwrap(), "updated");
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
        let result = tokio::time::timeout(Duration::from_secs(3), client.fetch_text(&url))
            .await
            .unwrap();
        server.join().unwrap();
        assert!(matches!(result, Err(ScraperError::AccessBlocked(_))));
    }

    #[test]
    fn tls_fallback_is_limited_to_mnd_host() {
        assert!(is_mnd_tls_fallback_host(
            "https://www.mnd.gov.tw/news/pressreleaselist"
        ));
        assert!(!is_mnd_tls_fallback_host(
            "https://mnd.gov.tw/news/pressreleaselist"
        ));
        assert!(!is_mnd_tls_fallback_host("https://example.test/"));
    }

    #[test]
    fn fast_fail_timeout_is_limited_to_wda_host() {
        assert!(is_wda_fast_fail_host(
            "https://www.wda.gov.tw/OpenData.aspx?SN=8C4FEB29449A1601"
        ));
        assert!(!is_wda_fast_fail_host("https://www.mol.gov.tw/"));
    }

    #[test]
    fn fast_primary_timeout_is_limited_to_nics_host() {
        assert!(is_nics_primary_host(
            "https://www.nics.nat.gov.tw/latest_news/announcements/Latest_Announcement/"
        ));
        assert!(!is_nics_primary_host("https://www.nat.gov.tw/"));
    }

    #[test]
    fn detail_fast_fail_is_limited_to_slow_full_text_hosts() {
        assert!(is_detail_fast_fail_host(
            "https://www.moj.gov.tw/2204/2205/2206/"
        ));
        assert!(is_detail_fast_fail_host(
            "https://www.nps.gov.tw/ch/titlelist/parknews/1"
        ));
        assert!(is_detail_fast_fail_host(
            "https://www.moa.gov.tw/theme_data.php?theme=news&sub_theme=agri&id=1"
        ));
        assert!(!is_detail_fast_fail_host("https://www.mof.gov.tw/"));
    }

    #[test]
    fn completes_a_truncated_json_array_at_the_last_whole_item() {
        let body = br#"[{"id":1,"text":"brace } and escaped \" quote"},{"id":2},{"id":3"#;
        let completed = complete_json_array_prefix(body).expect("complete prefix");
        assert_eq!(
            completed,
            r#"[{"id":1,"text":"brace } and escaped \" quote"},{"id":2}]"#
        );
        let parsed: serde_json::Value = serde_json::from_str(&completed).expect("valid JSON");
        assert_eq!(parsed.as_array().map(Vec::len), Some(2));
    }
}
