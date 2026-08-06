use super::ScraperError;
use reqwest::{Client, StatusCode};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, ScraperError> {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent("taiwan-government-news/2.0")
            .gzip(true)
            .build()
            .map_err(|error| {
                ScraperError::Unknown(format!("HTTP client initialization failed: {error}"))
            })?;
        Ok(Self { client })
    }

    pub async fn fetch_text(&self, url: &str) -> Result<String, ScraperError> {
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
            .map_err(|error| ScraperError::ParserRegression(error.to_string()))
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
