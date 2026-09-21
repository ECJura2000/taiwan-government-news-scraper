use super::ScraperError;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, PRAGMA},
    Client, StatusCode,
};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
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
            .default_headers(headers)
            .gzip(true)
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("MND TLS fallback client initialization failed: {error}"))
            })?;
        Ok(Self {
            client,
            mnd_tls_fallback_client,
        })
    }

    pub async fn fetch_text(&self, url: &str) -> Result<String, ScraperError> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.fetch_once(&self.client, url).await {
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
                Err(error) if error.retryable() && attempt < 2 => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ScraperError::Unknown("HTTP retry exhausted".into())))
    }

    async fn fetch_once(&self, client: &Client, url: &str) -> Result<String, ScraperError> {
        let response = client.get(url).send().await.map_err(|error| {
            let message = error.to_string();
            if message.to_ascii_lowercase().contains("certificate") {
                ScraperError::TlsCertificate(message)
            } else if error.is_timeout() || error.is_connect() {
                ScraperError::RunnerNetwork(error.to_string())
            } else if error.is_request() {
                ScraperError::SourceOutage(error.to_string())
            } else {
                ScraperError::Unknown(error.to_string())
            }
        })?;
        let status = response.status();
        if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
            return Err(ScraperError::AccessBlocked(format!("HTTP {status}")));
        }
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(ScraperError::SourceOutage(format!("HTTP {status}")));
        }
        if !status.is_success() {
            return Err(ScraperError::ParserRegression(format!("HTTP {status}")));
        }
        response
            .text()
            .await
            .map_err(|error| ScraperError::SourceOutage(format!("response body: {error:?}")))
    }
}

fn is_mnd_tls_fallback_host(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .as_deref()
        == Some("www.mnd.gov.tw")
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
}
