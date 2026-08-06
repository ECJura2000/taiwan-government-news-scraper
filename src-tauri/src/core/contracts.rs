use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsItem {
    pub source: String,
    pub date: String,
    pub department: String,
    pub title: String,
    pub link: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default = "default_date_source")]
    pub date_source: String,
}

fn default_date_source() -> String {
    "published".to_owned()
}

impl NewsItem {
    pub fn normalized_summary(value: &str, max_length: usize) -> String {
        let mut text = value
            .replace("\r", " ")
            .replace("\n", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.len() > max_length {
            text.truncate(max_length);
            while !text.is_char_boundary(text.len()) {
                text.pop();
            }
        }
        text
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    SourceOutage,
    RunnerNetwork,
    TlsCertificate,
    AccessBlocked,
    ParserRegression,
    BrowserRuntime,
    Unknown,
}

impl FailureClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceOutage => "source_outage",
            Self::RunnerNetwork => "runner_network",
            Self::TlsCertificate => "tls_certificate",
            Self::AccessBlocked => "access_blocked",
            Self::ParserRegression => "parser_regression",
            Self::BrowserRuntime => "browser_runtime",
            Self::Unknown => "unknown",
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::SourceOutage | Self::RunnerNetwork | Self::TlsCertificate
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Success,
    Attention,
    PartialFailure,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReportContract {
    pub status: String,
    #[serde(default)]
    pub failed_sources: Vec<String>,
    #[serde(default)]
    pub anomalies: Vec<String>,
    #[serde(default)]
    pub failure_class_counts: serde_json::Value,
    #[serde(default)]
    pub source_health: serde_json::Value,
    #[serde(default)]
    pub quality: serde_json::Value,
    #[serde(default)]
    pub relevance_policy: serde_json::Value,
    #[serde(default)]
    pub output_file: String,
    #[serde(default)]
    pub report_file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_normalization_matches_whitespace_contract() {
        assert_eq!(
            NewsItem::normalized_summary("  一\n二\r\n三  ", 4000),
            "一 二 三"
        );
    }

    #[test]
    fn failure_classes_preserve_retry_policy() {
        assert!(FailureClass::RunnerNetwork.retryable());
        assert!(FailureClass::SourceOutage.retryable());
        assert!(!FailureClass::ParserRegression.retryable());
        assert_eq!(FailureClass::BrowserRuntime.as_str(), "browser_runtime");
    }

    #[test]
    fn report_contract_accepts_legacy_optional_fields() {
        let report: ReportContract = serde_json::from_str(
            r#"{
            "status":"attention",
            "quality":{"alert_reasons":[]},
            "output_file":"result.xlsx"
        }"#,
        )
        .unwrap();
        assert_eq!(report.status, "attention");
        assert_eq!(report.report_file, "");
    }
}
