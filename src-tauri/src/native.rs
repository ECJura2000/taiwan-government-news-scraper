use crate::scraper::catalog::{all_sources, find_source};
use crate::scraper::http::HttpClient;
use crate::scraper::{html, rss, NewsItem, ScraperError};
use chrono::{Datelike, Local, NaiveDate};
use futures::stream::{self, StreamExt};
use regex::Regex;
use rust_xlsxwriter::Workbook;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const DEFAULT_OUTPUT_DIR: &str = "新聞搜集區";

#[derive(Debug)]
struct SourceResult {
    source: String,
    items: Vec<NewsItem>,
    error: Option<ScraperError>,
}

pub async fn run(
    options: &crate::RunOptions,
    cancelled: Arc<AtomicBool>,
) -> Result<crate::RunSummary, String> {
    let selected: Vec<String> = if options.sources.is_empty() {
        all_sources()
            .iter()
            .map(|source| source.name.clone())
            .collect()
    } else {
        options.sources.clone()
    };
    let max_workers = options.max_workers.max(1) as usize;
    let client = HttpClient::new().map_err(|error| error.to_string())?;
    let results: Vec<SourceResult> = stream::iter(selected.iter().cloned())
        .map(|source| {
            let client = client.clone();
            let cancelled = cancelled.clone();
            async move {
                if cancelled.load(Ordering::SeqCst) {
                    return SourceResult {
                        source,
                        items: Vec::new(),
                        error: Some(ScraperError::Unknown("執行已取消".into())),
                    };
                }
                fetch_source(&client, &source).await
            }
        })
        .buffer_unordered(max_workers)
        .collect()
        .await;

    if cancelled.load(Ordering::SeqCst) {
        return Err("執行已取消".into());
    }
    let mut items = Vec::new();
    let mut failed_sources = Vec::new();
    let mut failure_class_counts: HashMap<String, u64> = HashMap::new();
    for result in &results {
        items.extend(result.items.clone());
        if let Some(error) = &result.error {
            failed_sources.push(result.source.clone());
            *failure_class_counts
                .entry(error.failure_class().as_str().to_owned())
                .or_default() += 1;
        }
    }
    let input_count = items.len();
    items = crate::scraper::quality::dedupe(items);
    let duplicate_count = input_count.saturating_sub(items.len());
    let paths = write_outputs(options, &items, &failed_sources, duplicate_count)?;
    let status = if failed_sources.is_empty() {
        "success"
    } else if options.fail_on_source_error {
        "failure"
    } else {
        "partial_failure"
    };
    Ok(crate::RunSummary {
        status: status.into(),
        news_count: items.len() as u64,
        failed_sources,
        anomalies: Vec::new(),
        failure_class_counts: json!(failure_class_counts),
        source_health: json!({
            "healthy_count": results.iter().filter(|result| result.error.is_none()).count(),
            "unstable_count": 0,
            "failed_count": results.iter().filter(|result| result.error.is_some()).count(),
            "fallback_source_count": 0
        }),
        quality: json!({
            "input_count": input_count,
            "output_count": items.len(),
            "duplicate_count": duplicate_count,
            "invalid_count": 0,
            "excluded_non_news_count": 0,
            "alert_reasons": []
        }),
        relevance_policy: json!({
            "engine": "rust-native-pending-parity",
            "ruleset_hash": ""
        }),
        output_file: paths.0.to_string_lossy().into_owned(),
        report_file: paths.1.to_string_lossy().into_owned(),
        error: None,
    })
}

async fn fetch_source(client: &HttpClient, source: &str) -> SourceResult {
    let Some(definition) = find_source(source) else {
        return SourceResult {
            source: source.to_owned(),
            items: Vec::new(),
            error: Some(ScraperError::Unknown("來源未在 Rust catalog 註冊".into())),
        };
    };
    let mut last_error = None;
    for url in &definition.urls {
        match client.fetch_text(url).await {
            Ok(body) => {
                let parsed = if looks_like_feed(url, &body) {
                    rss::parse_feed(source, &body)
                } else {
                    html::parse_link_list(source, &body, Some(url), "a[href]")
                };
                match parsed {
                    Ok(items) => {
                        return SourceResult {
                            source: source.to_owned(),
                            items: filter_to_current_week(items),
                            error: None,
                        };
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    SourceResult {
        source: source.to_owned(),
        items: Vec::new(),
        error: last_error.or_else(|| Some(ScraperError::SourceOutage("沒有可用來源入口".into()))),
    }
}

fn looks_like_feed(url: &str, body: &str) -> bool {
    let lower_url = url.to_ascii_lowercase();
    let trimmed = body.trim_start().to_ascii_lowercase();
    lower_url.contains("rss")
        || lower_url.contains("feed")
        || trimmed.starts_with("<?xml")
        || trimmed.starts_with("<rss")
        || trimmed.starts_with("<feed")
}

fn filter_to_current_week(items: Vec<NewsItem>) -> Vec<NewsItem> {
    let today = Local::now().date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let sunday = monday + chrono::Duration::days(6);
    items
        .into_iter()
        .filter(|item| {
            item.date.is_empty()
                || parse_date(&item.date)
                    .map(|date| date >= monday && date <= sunday)
                    .unwrap_or(false)
        })
        .collect()
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    for format in ["%Y-%m-%d", "%Y/%m/%d", "%Y年%m月%d日"] {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date);
        }
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.date_naive())
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|value| value.date_naive())
        })
        .or_else(|| {
            Regex::new(r"(20\d{2})[-/]([01]?\d)[-/]([0-3]?\d)")
                .ok()?
                .captures(value)
                .and_then(|capture| {
                    NaiveDate::from_ymd_opt(
                        capture.get(1)?.as_str().parse().ok()?,
                        capture.get(2)?.as_str().parse().ok()?,
                        capture.get(3)?.as_str().parse().ok()?,
                    )
                })
        })
}

fn write_outputs(
    options: &crate::RunOptions,
    items: &[NewsItem],
    failed_sources: &[String],
    duplicate_count: usize,
) -> Result<(PathBuf, PathBuf), String> {
    let output_dir = PathBuf::from(options.output_dir.as_deref().unwrap_or(DEFAULT_OUTPUT_DIR));
    let report_dir = PathBuf::from(
        options
            .report_dir
            .as_deref()
            .unwrap_or("新聞搜集區/執行紀錄"),
    );
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&report_dir).map_err(|error| error.to_string())?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let workbook_path = output_dir.join(format!("本週新聞整理_rust_{stamp}.xlsx"));
    let report_path = report_dir.join(format!("news_scraper_rust_{stamp}.json"));

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    for (column, title) in ["來源", "日期", "單位", "標題", "連結", "摘要"]
        .iter()
        .enumerate()
    {
        worksheet
            .write_string(0, column as u16, *title)
            .map_err(|error| error.to_string())?;
    }
    for (row, item) in items.iter().enumerate() {
        let row = (row + 1) as u32;
        for (column, value) in [
            &item.source,
            &item.date,
            &item.department,
            &item.title,
            &item.link,
            &item.summary,
        ]
        .iter()
        .enumerate()
        {
            worksheet
                .write_string(row, column as u16, *value)
                .map_err(|error| error.to_string())?;
        }
    }
    workbook
        .save(&workbook_path)
        .map_err(|error| error.to_string())?;
    let report = json!({
        "schema_version": 1,
        "engine": "rust-native",
        "status": if failed_sources.is_empty() { "success" } else { "partial_failure" },
        "news_count": items.len(),
        "failed_sources": failed_sources,
        "quality": {"input_count": items.len() + duplicate_count, "output_count": items.len(), "duplicate_count": duplicate_count, "alert_reasons": []},
        "output_file": workbook_path,
        "report_file": report_path
    });
    std::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok((workbook_path, report_path))
}

#[allow(dead_code)]
fn _path_exists(path: &Path) -> bool {
    path.exists()
}
