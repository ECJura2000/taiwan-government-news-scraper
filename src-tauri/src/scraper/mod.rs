pub mod adapters;
pub mod catalog;
pub mod html;
pub mod http;
pub mod quality;
pub mod rss;
pub mod scheduler;
pub mod special;

use crate::core::FailureClass;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScraperError {
    #[error("來源暫時故障：{0}")]
    SourceOutage(String),
    #[error("執行器網路錯誤：{0}")]
    RunnerNetwork(String),
    #[error("TLS 憑證錯誤：{0}")]
    TlsCertificate(String),
    #[error("來源拒絕存取：{0}")]
    AccessBlocked(String),
    #[error("來源解析錯誤：{0}")]
    ParserRegression(String),
    #[error("瀏覽器執行錯誤：{0}")]
    BrowserRuntime(String),
    #[error("未知來源錯誤：{0}")]
    Unknown(String),
}

impl ScraperError {
    pub const fn failure_class(&self) -> FailureClass {
        match self {
            Self::SourceOutage(_) => FailureClass::SourceOutage,
            Self::RunnerNetwork(_) => FailureClass::RunnerNetwork,
            Self::TlsCertificate(_) => FailureClass::TlsCertificate,
            Self::AccessBlocked(_) => FailureClass::AccessBlocked,
            Self::ParserRegression(_) => FailureClass::ParserRegression,
            Self::BrowserRuntime(_) => FailureClass::BrowserRuntime,
            Self::Unknown(_) => FailureClass::Unknown,
        }
    }

    pub const fn retryable(&self) -> bool {
        self.failure_class().retryable()
    }

    pub const fn error_category(&self) -> &'static str {
        match self {
            Self::SourceOutage(_) => "connection",
            Self::RunnerNetwork(_) => "connection",
            Self::TlsCertificate(_) => "ssl",
            Self::AccessBlocked(_) => "http",
            Self::ParserRegression(_) => "parse",
            Self::BrowserRuntime(_) => "browser",
            Self::Unknown(_) => "unexpected",
        }
    }
}

pub use crate::core::NewsItem;
