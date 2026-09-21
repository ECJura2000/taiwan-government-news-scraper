use super::ScraperError;
use reqwest::{
    header::{
        HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, PRAGMA,
        RANGE,
    },
    Client, StatusCode,
};
use std::time::Duration;

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
        })
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
            match self.fetch_once(client, url).await {
                Ok(body) => return Ok(body),
                Err(error)
                    if is_mnd_tls_fallback_host(url)
                        && matches!(
                            error,
                            ScraperError::TlsCertificate(_) | ScraperError::RunnerNetwork(_)
                        ) =>
                {
                    return self.fetch_once(&self.mnd_tls_fallback_client, url).await;
                }
                Err(error) if error.retryable() && attempt + 1 < attempts => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ScraperError::Unknown("HTTP retry exhausted".into())))
    }

    pub async fn fetch_detail_text(&self, url: &str) -> Result<String, ScraperError> {
        if is_detail_fast_fail_host(url) {
            self.fetch_once(&self.detail_fast_fail_client, url).await
        } else {
            self.fetch_text(url).await
        }
    }

    pub async fn fetch_recent_json_array(&self, url: &str) -> Result<String, ScraperError> {
        let response = self
            .client
            .get(url)
            .header(RANGE, format!("bytes=0-{RECENT_JSON_PREFIX_END}"))
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        validate_status(status)?;
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))?;
        complete_json_array_prefix(&bytes)
    }

    async fn fetch_once(&self, client: &Client, url: &str) -> Result<String, ScraperError> {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        validate_status(status)?;
        response
            .text()
            .await
            .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))
    }
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
