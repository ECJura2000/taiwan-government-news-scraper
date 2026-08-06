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
            .default_headers(headers)
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("HTTP client initialization failed: {error}"))
            })?;
        Ok(Self { client })
    }

    pub async fn fetch_text(&self, url: &str) -> Result<String, ScraperError> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.fetch_once(url).await {
                Ok(body) => return Ok(body),
                Err(error) if error.retryable() && attempt < 2 => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| ScraperError::Unknown("HTTP retry exhausted".into())))
    }

    async fn fetch_once(&self, url: &str) -> Result<String, ScraperError> {
        let response = self.client.get(url).send().await.map_err(|error| {
            if error.is_timeout() || error.is_connect() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_can_be_constructed() {
        assert!(HttpClient::new().is_ok());
    }
}
