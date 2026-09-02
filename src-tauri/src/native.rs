use crate::scraper::adapters;
use crate::scraper::catalog::{all_sources, find_source, routes_for, SourceRoute};
use crate::scraper::http::HttpClient;
use crate::scraper::{NewsItem, ScraperError};
use chrono::{Datelike, Local, NaiveDate, Utc, Weekday};
use chrono_tz::Asia::Taipei;
use futures::stream::{self, StreamExt};
use regex::Regex;
use rust_xlsxwriter::{Color, DataValidation, Format, FormatAlign, Workbook};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_OUTPUT_DIR: &str = "新聞搜集區";

pub type ProgressCallback = Arc<dyn Fn(crate::ProgressEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug)]
struct SourceResult {
    source: String,
    items: Vec<NewsItem>,
    error: Option<ScraperError>,
    attempts: Vec<serde_json::Value>,
    final_route: Option<serde_json::Value>,
}

pub async fn run(
    options: &crate::RunOptions,
    cancelled: Arc<AtomicBool>,
) -> Result<crate::RunSummary, String> {
    run_with_progress(options, cancelled, None).await
}

pub async fn run_with_progress(
    options: &crate::RunOptions,
    cancelled: Arc<AtomicBool>,
    progress: Option<ProgressCallback>,
) -> Result<crate::RunSummary, String> {
    let started_at = Utc::now();
    let selected: Vec<String> = if options.sources.is_empty() {
        all_sources()
            .iter()
            .map(|source| source.name.clone())
            .collect()
    } else {
        options.sources.clone()
    };
    let max_workers = options.max_workers.max(1) as usize;
    let total = selected.len() as u32;
    let date_range = resolve_date_range(options)?;
    let client = HttpClient::new().map_err(|error| error.to_string())?;
    let mut jobs = stream::iter(selected.iter().cloned())
        .map(|source| {
            let client = client.clone();
            let cancelled = cancelled.clone();
            let progress = progress.clone();
            async move {
                if cancelled.load(Ordering::SeqCst) {
                    return SourceResult {
                        source,
                        items: Vec::new(),
                        error: Some(ScraperError::Unknown("執行已取消".into())),
                        attempts: Vec::new(),
                        final_route: None,
                    };
                }
                if let Some(progress) = &progress {
                    progress(crate::ProgressEvent {
                        kind: "source_started".into(),
                        source: Some(source.clone()),
                        completed: None,
                        total: Some(total),
                        message: Some(format!("正在處理：{source}")),
                    });
                }
                fetch_source(&client, &source, date_range, progress.as_ref(), total).await
            }
        })
        .buffer_unordered(max_workers);
    let mut results = Vec::with_capacity(selected.len());
    let mut completed = 0_u32;
    loop {
        tokio::select! {
            result = jobs.next() => {
                match result {
                    Some(result) => {
                        completed += 1;
                        if let Some(progress) = &progress {
                            let source = result.source.clone();
                            let failed = result.error.is_some();
                            progress(crate::ProgressEvent {
                                kind: if failed { "source_failed" } else { "source_finished" }.into(),
                                source: Some(source.clone()),
                                completed: Some(completed),
                                total: Some(total),
                                message: Some(if failed {
                                    format!("來源失敗：{source}")
                                } else {
                                    format!("完成來源：{source}")
                                }),
                            });
                        }
                        results.push(result);
                    },
                    None => break,
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if cancelled.load(Ordering::SeqCst) {
                    return Err("執行已取消".into());
                }
            }
        }
    }
    drop(jobs);

    if cancelled.load(Ordering::SeqCst) {
        return Err("執行已取消".into());
    }
    if let Some(progress) = &progress {
        progress(crate::ProgressEvent {
            kind: "writing_outputs".into(),
            source: None,
            completed: Some(completed),
            total: Some(total),
            message: Some("正在產生 Excel 與 JSON 報告".into()),
        });
    }
    let mut items = Vec::new();
    let mut failed_sources = Vec::new();
    let mut failure_class_counts: HashMap<String, u64> = HashMap::new();
    let mut error_counts: HashMap<String, u64> = HashMap::new();
    let mut source_attempts = Vec::new();
    let mut source_diagnostics = Vec::new();
    let mut route_attempts = Vec::new();
    let mut source_counts: HashMap<String, usize> = HashMap::new();
    for result in &results {
        items.extend(result.items.clone());
        source_counts.insert(result.source.clone(), result.items.len());
        source_attempts.extend(result.attempts.clone());
        route_attempts.extend(result.attempts.clone());
        if let Some(error) = &result.error {
            failed_sources.push(result.source.clone());
            *failure_class_counts
                .entry(error.failure_class().as_str().to_owned())
                .or_default() += 1;
        }
        for attempt in &result.attempts {
            if let Some(category) = attempt
                .get("error_category")
                .and_then(|value| value.as_str())
            {
                if !category.is_empty() {
                    *error_counts.entry(category.to_owned()).or_default() += 1;
                }
            }
        }
        source_diagnostics.push(source_diagnostic(result));
    }
    if options.dedupe_affiliated {
        items = crate::scraper::quality::dedupe_affiliated(items);
    }
    let quality_result = crate::scraper::quality::process(items);
    let input_count = quality_result.input_count;
    let duplicate_count = quality_result.duplicate_count;
    let invalid_count = quality_result.invalid_count;
    let excluded_non_news_count = quality_result.excluded_non_news_count;
    let issues = quality_result.issues;
    items = quality_result.items;
    source_counts.clear();
    for source in &selected {
        source_counts.insert(source.clone(), 0);
    }
    for item in &items {
        *source_counts.entry(item.source.clone()).or_default() += 1;
    }
    let paths = write_outputs(options, &items, duplicate_count, date_range)?;
    let finished_at = Utc::now();
    let duplicate_ratio = duplicate_count as f64 / input_count.max(1) as f64;
    let excluded_ratio = excluded_non_news_count as f64 / input_count.max(1) as f64;
    let mut alert_reasons = Vec::new();
    if invalid_count >= 1 {
        alert_reasons.push("invalid_items");
    }
    if duplicate_count >= 5 && duplicate_ratio >= 0.20 {
        alert_reasons.push("duplicate_spike");
    }
    if excluded_non_news_count >= 3 && excluded_ratio >= 0.25 {
        alert_reasons.push("non_news_spike");
    }
    let status = if !failed_sources.is_empty() {
        "partial_failure"
    } else if !alert_reasons.is_empty() {
        "attention"
    } else {
        "success"
    };
    let relevance_policy = crate::relevance::default_summary();
    let summary_count = items.iter().filter(|item| !item.summary.is_empty()).count();
    let mut date_source_counts: HashMap<String, usize> = HashMap::new();
    for item in &items {
        *date_source_counts
            .entry(item.date_source.clone())
            .or_default() += 1;
    }
    let summary_coverage_rate = if items.is_empty() {
        0.0
    } else {
        ((summary_count as f64 / items.len() as f64) * 10_000.0).round() / 10_000.0
    };
    let summary = crate::RunSummary {
        status: status.into(),
        news_count: items.len() as u64,
        failed_sources,
        anomalies: Vec::new(),
        failure_class_counts: json!(failure_class_counts),
        source_health: json!({
            "healthy_count": source_diagnostics.iter().filter(|result| result.get("status").and_then(|value| value.as_str()) == Some("success") && result.get("unstable").and_then(|value| value.as_bool()) != Some(true)).count(),
            "unstable_count": source_diagnostics.iter().filter(|result| result.get("unstable").and_then(|value| value.as_bool()) == Some(true)).count(),
            "failed_count": results.iter().filter(|result| result.error.is_some()).count(),
            "fallback_source_count": source_diagnostics.iter().filter(|result| result.get("final_route").and_then(|route| route.get("used_fallback")).and_then(|value| value.as_bool()) == Some(true)).count(),
            "coverage_reduced_count": 0,
            "ssl_fallback_host_count": 0
        }),
        quality: json!({
            "input_count": input_count,
            "output_count": items.len(),
            "duplicate_count": duplicate_count,
            "invalid_count": invalid_count,
            "excluded_non_news_count": excluded_non_news_count,
            "source_counts": source_counts,
            "summary_count": summary_count,
            "summary_coverage_rate": summary_coverage_rate,
            "date_source_counts": date_source_counts,
            "description_fallback_count": 0,
            "issues": issues,
            "alert_reasons": alert_reasons
        }),
        relevance_policy: relevance_policy.clone(),
        engine: "rust-native".into(),
        ai_policy: json!({
            "version": relevance_policy["template_version"],
            "ruleset_hash": relevance_policy["ruleset_hash"]
        }),
        output_file: paths.0.to_string_lossy().into_owned(),
        report_file: paths.1.to_string_lossy().into_owned(),
        week_start: date_range.start.to_string(),
        week_end: date_range.end.to_string(),
        report_schema_version: 4,
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        duration_seconds: (finished_at - started_at).num_milliseconds() as f64 / 1000.0,
        selected_source_count: selected.len() as u64,
        selected_sources: selected,
        error_counts: json!(error_counts),
        parser_warnings: json!([]),
        scheduling_plan: json!([]),
        alerts: json!([]),
        source_attempts: json!(source_attempts),
        source_diagnostics: json!(source_diagnostics),
        route_attempts: json!(route_attempts),
        insecure_ssl_hosts: Vec::new(),
        error: None,
    };
    let mut report_value = serde_json::to_value(&summary).map_err(|error| error.to_string())?;
    if let Some(report) = report_value.as_object_mut() {
        report.remove("engine");
        report.remove("report_file");
        report.remove("error");
    }
    let report = serde_json::to_vec_pretty(&report_value).map_err(|error| error.to_string())?;
    std::fs::write(&paths.1, report).map_err(|error| error.to_string())?;
    Ok(summary)
}

pub fn resolve_date_range(options: &crate::RunOptions) -> Result<DateRange, String> {
    if options.date.is_some() && (options.start_date.is_some() || options.end_date.is_some()) {
        return Err("--date 不可與 --start-date/--end-date 同時使用".into());
    }
    if options.start_date.is_some() != options.end_date.is_some() {
        return Err("--start-date 與 --end-date 必須同時提供".into());
    }
    if let Some(date) = &options.date {
        let date = parse_date_argument(date)?;
        return Ok(week_range_for_date(date));
    }
    if let (Some(start), Some(end)) = (&options.start_date, &options.end_date) {
        let start = parse_date_argument(start)?;
        let end = parse_date_argument(end)?;
        if start > end {
            return Err("--start-date 不可晚於 --end-date".into());
        }
        return Ok(DateRange { start, end });
    }

    Ok(default_range_for_date(
        Local::now().with_timezone(&Taipei).date_naive(),
    ))
}

fn week_range_for_date(date: NaiveDate) -> DateRange {
    let start = date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64);
    DateRange {
        start,
        end: start + chrono::Duration::days(6),
    }
}

fn default_range_for_date(today: NaiveDate) -> DateRange {
    let mut start = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    if today.weekday() == Weekday::Mon {
        start -= chrono::Duration::days(7);
    }
    DateRange {
        start,
        end: start + chrono::Duration::days(6),
    }
}

fn parse_date_argument(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("日期格式必須是 YYYY-MM-DD：{value}"))
}

async fn fetch_source(
    client: &HttpClient,
    source: &str,
    date_range: DateRange,
    progress: Option<&ProgressCallback>,
    total: u32,
) -> SourceResult {
    let Some(definition) = find_source(source) else {
        return SourceResult {
            source: source.to_owned(),
            items: Vec::new(),
            error: Some(ScraperError::Unknown("來源未在 Rust catalog 註冊".into())),
            attempts: Vec::new(),
            final_route: None,
        };
    };
    let mut last_error = None;
    let mut attempts = Vec::new();
    let mut aggregated_items = Vec::new();
    let mut successful_routes = 0usize;
    let mut aggregate_final_route = None;
    'routes: for route in routes_for(definition) {
        let index = route.priority.saturating_sub(1) as usize;
        let url = &route.url;
        let host = url::Url::parse(url)
            .ok()
            .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_default();
        for attempt_number in 1..=2 {
            let started = Instant::now();
            let fetched = if route.kind == "browser" {
                crate::browser::fetch_rendered_html_after(
                    url,
                    browser_page_script(route.parser.as_str()),
                )
                .await
                .map_err(ScraperError::BrowserRuntime)
            } else {
                client.fetch_text(url).await
            };
            let outcome = fetched.and_then(|body| adapters::parse_route(source, &route, &body));
            match outcome {
                Ok(items) => {
                    let parsed_item_count = items.len();
                    let filtered_items = filter_to_date_range(items, date_range);
                    let filtered_items =
                        enrich_detail_summaries(client, source, filtered_items).await;
                    attempts.push(json!({
                        "source": source,
                        "route_id": route.id,
                        "url": url,
                        "url_host": host,
                        "route_kind": route.kind.as_str(),
                        "parser": route.parser.as_str(),
                        "attempt_number": attempt_number,
                        "status": "success",
                        "elapsed_seconds": elapsed_seconds(started),
                        "item_count": parsed_item_count,
                        "failure_class": "",
                        "error_category": "",
                        "failure_evidence": {},
                    }));
                    if definition.aggregate_routes {
                        successful_routes += 1;
                        aggregated_items.extend(filtered_items);
                        aggregate_final_route = Some(json!({
                            "route_id": route.id,
                            "url": url,
                            "url_host": host,
                            "used_fallback": false,
                            "coverage_reduced": route.coverage_reduced,
                            "aggregated": true,
                        }));
                        continue 'routes;
                    }
                    return SourceResult {
                        source: source.to_owned(),
                        items: filtered_items,
                        error: None,
                        final_route: Some(json!({
                            "route_id": route.id,
                            "url": url,
                            "url_host": host,
                            "used_fallback": index > 0,
                            "coverage_reduced": route.coverage_reduced,
                        })),
                        attempts,
                    };
                }
                Err(error) => {
                    let should_retry = should_retry_browser_route(&route, &error, attempt_number);
                    attempts.push(attempt_json(
                        source,
                        &route,
                        &host,
                        started,
                        attempt_number,
                        &error,
                    ));
                    last_error = Some(error);
                    if should_retry {
                        if let Some(progress) = progress {
                            progress(crate::ProgressEvent {
                                kind: "retry".into(),
                                source: Some(source.to_owned()),
                                completed: None,
                                total: Some(total),
                                message: Some(format!("頁面第一次載入未完成，正在重試：{source}")),
                            });
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                        continue;
                    }
                    break;
                }
            }
        }
    }
    if successful_routes > 0 {
        return SourceResult {
            source: source.to_owned(),
            items: aggregated_items,
            error: None,
            attempts,
            final_route: aggregate_final_route,
        };
    }
    SourceResult {
        source: source.to_owned(),
        items: Vec::new(),
        error: last_error.or_else(|| Some(ScraperError::SourceOutage("沒有可用來源入口".into()))),
        attempts,
        final_route: None,
    }
}

fn browser_page_script(parser: &str) -> Option<&'static str> {
    match parser {
        "sports-html" => Some(
            "(async () => { const select = document.querySelector('#InputPageSize'); if (select) { select.value = '500'; select.dispatchEvent(new Event('change', { bubbles: true })); } const deadline = Date.now() + 20000; while (Date.now() < deadline) { const date = document.querySelector(\"tbody tr td[data-title='發布日期'] div.in, tbody tr td[data-title='上版日期'] div.in\"); if (date && date.textContent.trim()) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        "vghtpe-html" => Some(
            "(async () => { const deadline = Date.now() + 20000; while (Date.now() < deadline) { if (document.querySelector('table.stackedTable tbody tr, table tbody tr')) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        "moenv-html" => Some(
            "(async () => { const deadline = Date.now() + 20000; while (Date.now() < deadline) { if (document.querySelector('ul.list_group li, article.idx-news-card')) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        "moea-html" => Some(
            "(async () => { const deadline = Date.now() + 20000; while (Date.now() < deadline) { if (document.querySelector('#holderContent_grdNews tbody tr')) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        _ => None,
    }
}

fn should_retry_browser_route(
    route: &SourceRoute,
    error: &ScraperError,
    attempt_number: u32,
) -> bool {
    route.kind == "browser"
        && attempt_number == 1
        && matches!(
            error,
            ScraperError::BrowserRuntime(_) | ScraperError::ParserRegression(_)
        )
}

fn source_diagnostic(result: &SourceResult) -> serde_json::Value {
    let failed_attempts: Vec<&serde_json::Value> = result
        .attempts
        .iter()
        .filter(|attempt| attempt.get("status").and_then(|value| value.as_str()) == Some("failed"))
        .collect();
    let final_attempt = result.attempts.last().cloned().unwrap_or_else(|| json!({}));
    let last_failure = failed_attempts
        .last()
        .cloned()
        .cloned()
        .unwrap_or_else(|| json!({}));
    let unstable = result.error.is_none() && !failed_attempts.is_empty();
    json!({
        "source": result.source,
        "status": if result.error.is_some() { "failed" } else { "success" },
        "unstable": unstable,
        "item_count": result.items.len(),
        "attempt_count": result.attempts.len(),
        "failure_class": if result.error.is_some() { final_attempt.get("failure_class").cloned().unwrap_or(json!("unknown")) } else { json!("") },
        "last_failure_class": last_failure.get("failure_class").cloned().unwrap_or(json!("")),
        "error_category": if result.error.is_some() { final_attempt.get("error_category").cloned().unwrap_or(json!("unexpected")) } else { json!("") },
        "failure_evidence": last_failure.get("failure_evidence").cloned().unwrap_or(json!({})),
        "elapsed_seconds": result.attempts.iter().filter_map(|attempt| attempt.get("elapsed_seconds").and_then(|value| value.as_f64())).sum::<f64>(),
        "final_route": result.final_route.clone().unwrap_or_else(|| json!({})),
        "route_attempt_count": result.attempts.len(),
        "route_failure_classes": failed_attempts.iter().filter_map(|attempt| attempt.get("failure_class").and_then(|value| value.as_str())).collect::<Vec<_>>(),
    })
}

fn elapsed_seconds(started: Instant) -> f64 {
    (started.elapsed().as_millis() as f64 / 1000.0 * 1000.0).round() / 1000.0
}

fn attempt_json(
    source: &str,
    route: &crate::scraper::catalog::SourceRoute,
    host: &str,
    started: Instant,
    attempt_number: u32,
    error: &ScraperError,
) -> serde_json::Value {
    json!({
        "source": source,
        "route_id": route.id,
        "url": route.url,
        "url_host": host,
        "route_kind": route.kind,
        "parser": route.parser,
        "attempt_number": attempt_number,
        "status": "failed",
        "elapsed_seconds": elapsed_seconds(started),
        "item_count": 0,
        "failure_class": error.failure_class().as_str(),
        "error_category": error.error_category(),
        "failure_evidence": {
            "url_host": host,
            "message": error.to_string(),
        },
    })
}

fn filter_to_date_range(items: Vec<NewsItem>, date_range: DateRange) -> Vec<NewsItem> {
    items
        .into_iter()
        .filter_map(|mut item| {
            if item.date.is_empty() {
                return None;
            }
            parse_date(&item.date).and_then(|date| {
                if date >= date_range.start && date <= date_range.end {
                    item.date = date.to_string();
                    Some(item)
                } else {
                    None
                }
            })
        })
        .collect()
}

async fn enrich_detail_summaries(
    client: &HttpClient,
    source: &str,
    items: Vec<NewsItem>,
) -> Vec<NewsItem> {
    if !matches!(
        source,
        "數位發展部" | "數位產業署" | "資通安全署" | "國科會" | "經濟部"
    ) {
        return items;
    }
    stream::iter(items)
        .map(|mut item| {
            let client = client.clone();
            async move {
                if item.summary.is_empty() && !item.link.is_empty() {
                    if let Ok(body) = client.fetch_text(&item.link).await {
                        item.summary = adapters::parse_detail_summary(&item.source, &body);
                    }
                }
                item
            }
        })
        .buffered(4)
        .collect()
        .await
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    for format in ["%Y-%m-%d", "%Y/%m/%d", "%Y年%m月%d日"] {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date);
        }
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Taipei).date_naive())
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|value| value.with_timezone(&Taipei).date_naive())
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

const EXCEL_HEADERS: [&str; 15] = [
    "部會",
    "新聞日期",
    "單位分類",
    "新聞標題",
    "新聞連結",
    "新聞摘要",
    "日期來源",
    "關聯主題",
    "優先關聯機關",
    "關聯性",
    "關聯分數",
    "判定理由",
    "命中關鍵字",
    "排除關鍵字",
    "各主題評分",
];

const EXCEL_CELL_CHAR_LIMIT: usize = 32_767;
const EXCEL_LATIN_FONT: &str = "Times New Roman";
const EXCEL_CJK_FONT: &str = "標楷體";

fn bounded_excel_text(value: &str) -> String {
    value.chars().take(EXCEL_CELL_CHAR_LIMIT).collect()
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0x20000..=0x2A6DF
                | 0x2A700..=0x2B73F
                | 0x2B740..=0x2B81F
                | 0x2B820..=0x2CEAF
        )
    })
}

fn excel_row(item: &NewsItem) -> (Vec<String>, u32, String) {
    let result = crate::relevance::classify(&item.title, &item.source, &item.summary);
    let (parent_source, department_path) = excel_agency_path(&item.source, &item.department);
    let strings = |key: &str| {
        result[key]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join("、")
            })
            .unwrap_or_default()
    };
    let relevance = result["relevance"].as_str().unwrap_or("").to_owned();
    let score = result["score"].as_u64().unwrap_or(0) as u32;
    let reasons = result["reasons"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("；")
        })
        .unwrap_or_default();
    let topic_scores = result["topic_matches"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    Some(format!(
                        "{}（{}分，{}）",
                        value["name"].as_str()?,
                        value["score"].as_u64()?,
                        value["relevance"].as_str()?
                    ))
                })
                .collect::<Vec<_>>()
                .join("；")
        })
        .unwrap_or_default();
    let values = vec![
        parent_source,
        item.date.clone(),
        department_path,
        item.title.clone(),
        if item.link.starts_with("http://") || item.link.starts_with("https://") {
            format!("{}官網：{}", item.source, item.link)
        } else {
            item.link.clone()
        },
        item.summary.clone(),
        item.date_source.clone(),
        strings("topics"),
        strings("priority_sources"),
        relevance.clone(),
        score.to_string(),
        reasons,
        strings("matched_keywords"),
        strings("excluded_keywords"),
        topic_scores,
    ];
    (values, score, relevance)
}

fn excel_agency_path(source: &str, department: &str) -> (String, String) {
    let base = crate::scraper::quality::affiliated_path(source);
    let mut path = if base.is_empty() {
        vec![source.to_owned()]
    } else {
        base.iter().map(|value| (*value).to_owned()).collect()
    };
    for part in department
        .split('／')
        .flat_map(|value| value.split(" / "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !path.iter().any(|existing| existing == part) {
            path.push(part.to_owned());
        }
    }
    let parent = path.first().cloned().unwrap_or_else(|| source.to_owned());
    let department = path.into_iter().skip(1).collect::<Vec<_>>().join(" / ");
    (parent, department)
}

fn source_order(source: &str) -> usize {
    all_sources()
        .iter()
        .position(|definition| definition.name == source)
        .unwrap_or(usize::MAX)
}

fn extract_http_url(value: &str) -> Option<&str> {
    let start = value.find("https://").or_else(|| value.find("http://"))?;
    Some(&value[start..])
}

fn roc_date(value: &str) -> Option<String> {
    let date = parse_date(value)?;
    Some(format!(
        "民國{}/{}/{}",
        date.year() - 1911,
        date.month(),
        date.day()
    ))
}

struct ExcelFormats {
    header: Format,
    body: Format,
    latin_body: Format,
    high: Format,
    latin_high: Format,
    possible: Format,
    latin_possible: Format,
}

fn write_table_sheet(
    workbook: &mut Workbook,
    name: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    formats: &ExcelFormats,
) -> Result<(), String> {
    let worksheet = workbook
        .add_worksheet()
        .set_name(name)
        .map_err(|error| error.to_string())?;
    for (column, title) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *title, &formats.header)
            .map_err(|error| error.to_string())?;
    }
    for (row, values) in rows.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let value = bounded_excel_text(value);
            let cell_format = if contains_cjk(&value) {
                &formats.body
            } else {
                &formats.latin_body
            };
            worksheet
                .write_string_with_format((row + 1) as u32, column as u16, &value, cell_format)
                .map_err(|error| error.to_string())?;
        }
    }
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|error| error.to_string())?;
    worksheet
        .autofilter(
            0,
            0,
            rows.len() as u32,
            headers.len().saturating_sub(1) as u16,
        )
        .map_err(|error| error.to_string())?;
    worksheet
        .set_row_height(0, 22)
        .map_err(|error| error.to_string())?;
    for row in 1..=rows.len() as u32 {
        worksheet
            .set_row_height(row, 22)
            .map_err(|error| error.to_string())?;
    }
    let widths: &[f64] = match name {
        "主題規則對照" => &[
            34.0, 12.0, 28.0, 16.0, 16.0, 72.0, 72.0, 72.0, 72.0, 55.0, 55.0,
        ],
        "關聯性規則" => &[34.0, 16.0, 55.0, 18.0, 10.0, 12.0, 42.0],
        "規則版本" => &[28.0, 80.0],
        _ => &[],
    };
    for (column, width) in widths.iter().enumerate() {
        worksheet
            .set_column_width(column as u16, *width)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn json_string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(String::from)
        .collect()
}

fn policy_reference_rows(document: &serde_json::Value) -> Vec<Vec<String>> {
    let global_context = json_string_list(&document["general_keywords"]).join("、");
    let global_exclusions = json_string_list(&document["negative_keywords"]).join("、");
    document["initiatives"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|initiative| {
            vec![
                initiative["name"].as_str().unwrap_or("").into(),
                "是".into(),
                initiative["lead_source"].as_str().unwrap_or("").into(),
                "是".into(),
                "#FFFF00".into(),
                json_string_list(&initiative["strong_keywords"]).join("、"),
                json_string_list(&initiative["context_keywords"]).join("、"),
                String::new(),
                global_context.clone(),
                String::new(),
                global_exclusions.clone(),
            ]
        })
        .collect()
}

fn policy_rule_rows(document: &serde_json::Value) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for keyword in json_string_list(&document["general_keywords"]) {
        let id = crate::relevance::stable_default_id("global-context", &[&keyword]);
        rows.push(vec![
            "全域".into(),
            "脈絡詞".into(),
            keyword,
            "標題與摘要".into(),
            "是".into(),
            "default".into(),
            id,
        ]);
    }
    for initiative in document["initiatives"].as_array().into_iter().flatten() {
        let name = initiative["name"].as_str().unwrap_or("");
        for (kind, key, label) in [
            ("core", "strong_keywords", "核心詞"),
            ("supporting", "context_keywords", "輔助詞"),
        ] {
            for keyword in json_string_list(&initiative[key]) {
                let id = crate::relevance::stable_default_id(kind, &[name, &keyword]);
                rows.push(vec![
                    name.into(),
                    label.into(),
                    keyword,
                    "標題與摘要".into(),
                    "是".into(),
                    "default".into(),
                    id,
                ]);
            }
        }
    }
    for keyword in json_string_list(&document["negative_keywords"]) {
        let id = crate::relevance::stable_default_id("exclusion", &[&keyword]);
        rows.push(vec![
            "全域".into(),
            "排除詞".into(),
            keyword,
            "標題".into(),
            "是".into(),
            "default".into(),
            id,
        ]);
    }
    rows
}

fn policy_version_rows(summary: &serde_json::Value) -> Vec<Vec<String>> {
    [
        ("設定格式版本", "schema_version"),
        ("設定名稱", "name"),
        ("範本版本", "template_version"),
        ("有效規則雜湊", "ruleset_hash"),
        ("設定來源", "source"),
        ("主題總數", "topic_count"),
        ("啟用主題數", "enabled_topic_count"),
        ("停用主題數", "disabled_topic_count"),
        ("關鍵字總數", "keyword_count"),
        ("啟用關鍵字數", "enabled_keyword_count"),
        ("停用關鍵字數", "disabled_keyword_count"),
        ("排除詞總數", "exclusion_count"),
        ("啟用排除詞數", "enabled_exclusion_count"),
        ("停用排除詞數", "disabled_exclusion_count"),
        ("自訂項目數", "custom_item_count"),
        ("已刪除預設項目數", "deleted_default_count"),
    ]
    .into_iter()
    .map(|(label, key)| {
        vec![
            label.into(),
            summary[key]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| summary[key].to_string()),
        ]
    })
    .chain(std::iter::once(vec![
        "匯出時間".into(),
        Local::now().to_rfc3339(),
    ]))
    .collect()
}

fn write_news_sheet(
    workbook: &mut Workbook,
    name: &str,
    rows: &[(Vec<String>, u32, String)],
    formats: &ExcelFormats,
) -> Result<(), String> {
    let worksheet = workbook
        .add_worksheet()
        .set_name(name)
        .map_err(|error| error.to_string())?;
    for (column, title) in EXCEL_HEADERS.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *title, &formats.header)
            .map_err(|error| error.to_string())?;
    }
    let mut date_cells: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (row, (values, score, relevance)) in rows.iter().enumerate() {
        let row = (row + 1) as u32;
        for (column, value) in values.iter().enumerate() {
            let bounded_value = bounded_excel_text(value);
            let base_format = if contains_cjk(&bounded_value) {
                &formats.body
            } else {
                &formats.latin_body
            };
            let highlight_format = if contains_cjk(&bounded_value) {
                &formats.high
            } else {
                &formats.latin_high
            };
            let possible_relevance_format = if contains_cjk(&bounded_value) {
                &formats.possible
            } else {
                &formats.latin_possible
            };
            let cell_format = if column == 7 && !value.is_empty() {
                highlight_format
            } else if column == 10 {
                &formats.latin_body
            } else if relevance == "高度相關" {
                highlight_format
            } else if relevance == "可能相關" {
                possible_relevance_format
            } else {
                base_format
            };
            if column == 4 && extract_http_url(value).is_some() {
                let url = extract_http_url(value).expect("URL checked");
                worksheet
                    .write_url_with_text(row, column as u16, url, bounded_value.as_str())
                    .map_err(|error| error.to_string())?;
                worksheet
                    .set_cell_format(row, column as u16, cell_format)
                    .map_err(|error| error.to_string())?;
            } else if column == 10 {
                worksheet
                    .write_number_with_format(row, column as u16, *score as f64, cell_format)
                    .map_err(|error| error.to_string())?;
            } else {
                worksheet
                    .write_string_with_format(row, column as u16, &bounded_value, cell_format)
                    .map_err(|error| error.to_string())?;
            }
        }
        if let Some(roc) = roc_date(&values[1]) {
            date_cells
                .entry((values[1].clone(), roc))
                .or_default()
                .push(format!("B{}", row + 1));
        }
        worksheet
            .set_row_height(row, 22)
            .map_err(|error| error.to_string())?;
    }
    for ((date, roc), cells) in date_cells {
        let validation = DataValidation::new()
            .allow_list_strings(&[date.as_str(), roc.as_str()])
            .map_err(|error| error.to_string())?
            .set_input_title("新聞日期格式")
            .map_err(|error| error.to_string())?
            .set_input_message("可選擇西元紀年或民國紀年。")
            .map_err(|error| error.to_string())?
            .set_error_title("日期格式不正確")
            .map_err(|error| error.to_string())?
            .set_error_message("請選擇西元日期或民國日期。")
            .map_err(|error| error.to_string())?
            .set_multi_range(cells.join(" "));
        worksheet
            .add_data_validation(1, 1, 1, 1, &validation)
            .map_err(|error| error.to_string())?;
    }
    worksheet
        .set_row_height(0, 22)
        .map_err(|error| error.to_string())?;
    worksheet
        .set_freeze_panes(1, 0)
        .map_err(|error| error.to_string())?;
    worksheet
        .autofilter(0, 0, rows.len() as u32, (EXCEL_HEADERS.len() - 1) as u16)
        .map_err(|error| error.to_string())?;
    for (column, width) in [
        23.2, 28.0, 45.0, 120.0, 130.0, 90.0, 22.0, 36.0, 16.0, 14.0, 12.0, 65.0, 55.0, 32.0, 65.0,
    ]
    .iter()
    .enumerate()
    {
        worksheet
            .set_column_width(column as u16, *width)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_outputs(
    options: &crate::RunOptions,
    items: &[NewsItem],
    duplicate_count: usize,
    date_range: DateRange,
) -> Result<(PathBuf, PathBuf), String> {
    let output_dir = PathBuf::from(options.output_dir.as_deref().unwrap_or(DEFAULT_OUTPUT_DIR));
    let report_dir = options
        .report_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| output_dir.join("執行紀錄"));
    std::fs::create_dir_all(&output_dir).map_err(|error| {
        format!(
            "無法建立 Excel 輸出資料夾 {}：{error}",
            output_dir.display()
        )
    })?;
    std::fs::create_dir_all(&report_dir)
        .map_err(|error| format!("無法建立 JSON 報告資料夾 {}：{error}", report_dir.display()))?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S_%6f").to_string();
    let workbook_path = output_dir.join(format!(
        "本週新聞整理（{}至{}）.xlsx",
        roc_compact(date_range.start),
        roc_compact(date_range.end)
    ));
    let report_path = report_dir.join(format!("news_scraper_run_{stamp}.json"));

    let mut workbook = Workbook::new();
    let mut rows: Vec<(Vec<String>, u32, String)> = items.iter().map(excel_row).collect();
    rows.sort_by(|left, right| {
        source_order(&left.0[0])
            .cmp(&source_order(&right.0[0]))
            .then_with(|| left.0[1].cmp(&right.0[1]))
            .then_with(|| left.0[2].cmp(&right.0[2]))
            .then_with(|| left.0[3].cmp(&right.0[3]))
    });
    let mut selected_rows: Vec<(Vec<String>, u32, String)> = rows
        .iter()
        .filter(|(_, _, relevance)| relevance == "高度相關" || relevance == "可能相關")
        .cloned()
        .collect();
    selected_rows.sort_by(|left, right| {
        let rank = |relevance: &str| if relevance == "高度相關" { 0 } else { 1 };
        rank(&left.2)
            .cmp(&rank(&right.2))
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0[7].cmp(&right.0[7]))
    });
    let formats = ExcelFormats {
        header: Format::new()
            .set_bold()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_text_wrap(),
        body: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap(),
        latin_body: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap(),
        high: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::Yellow),
        latin_high: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::Yellow),
        possible: Format::new()
            .set_font_name(EXCEL_CJK_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::RGB(0xFFF2CC)),
        latin_possible: Format::new()
            .set_font_name(EXCEL_LATIN_FONT)
            .set_font_size(11)
            .set_align(FormatAlign::Top)
            .set_text_wrap()
            .set_background_color(Color::RGB(0xFFF2CC)),
    };
    write_news_sheet(&mut workbook, "全部新聞", &rows, &formats)?;
    write_news_sheet(&mut workbook, "已初步篩選工作表", &selected_rows, &formats)?;
    let policy_document = crate::relevance::policy_document();
    let reference_rows = policy_reference_rows(&policy_document);
    write_table_sheet(
        &mut workbook,
        "主題規則對照",
        &[
            "主題",
            "啟用",
            "優先關聯機關",
            "比對主題名稱",
            "顯示顏色",
            "核心詞",
            "輔助詞",
            "主題脈絡詞",
            "全域脈絡詞",
            "主題排除詞",
            "全域排除詞",
        ],
        &reference_rows,
        &formats,
    )?;
    let rule_rows = policy_rule_rows(&policy_document);
    write_table_sheet(
        &mut workbook,
        "關聯性規則",
        &[
            "主題",
            "規則類型",
            "關鍵字",
            "比對欄位",
            "啟用",
            "來源",
            "規則ID",
        ],
        &rule_rows,
        &formats,
    )?;
    let policy_summary = crate::relevance::default_summary();
    let version_rows = policy_version_rows(&policy_summary);
    write_table_sheet(
        &mut workbook,
        "規則版本",
        &["項目", "內容"],
        &version_rows,
        &formats,
    )?;
    for (source, sheet_name) in [
        ("財政部", "財政部"),
        ("國發會", "國發會"),
        ("國科會", "國科會"),
        ("數位發展部", "數發部"),
        ("經濟部", "經濟部"),
    ] {
        let source_rows: Vec<_> = rows
            .iter()
            .filter(|row| row.0[0] == source)
            .cloned()
            .collect();
        write_news_sheet(&mut workbook, sheet_name, &source_rows, &formats)?;
    }
    workbook
        .save(&workbook_path)
        .map_err(|error| error.to_string())?;
    let _ = duplicate_count;
    Ok((workbook_path, report_path))
}

fn roc_compact(date: NaiveDate) -> String {
    format!(
        "{:03}{:02}{:02}",
        date.year() - 1911,
        date.month(),
        date.day()
    )
}

#[allow(dead_code)]
fn _path_exists(path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn options() -> crate::RunOptions {
        crate::RunOptions {
            sources: Vec::new(),
            output_dir: None,
            report_dir: None,
            date: None,
            start_date: None,
            end_date: None,
            max_workers: 8,
            dedupe_affiliated: false,
            fail_on_source_error: false,
        }
    }

    fn browser_route(parser: &str) -> SourceRoute {
        SourceRoute {
            id: "official-browser".into(),
            url: "https://example.test/news".into(),
            kind: "browser".into(),
            parser: parser.into(),
            priority: 1,
            official: true,
            coverage_reduced: false,
        }
    }

    fn route_attempt(
        route_id: &str,
        attempt_number: u32,
        status: &str,
        failure_class: &str,
    ) -> serde_json::Value {
        json!({
            "source": "測試來源",
            "route_id": route_id,
            "url": format!("https://{route_id}.example.test/news"),
            "url_host": format!("{route_id}.example.test"),
            "route_kind": "browser",
            "parser": "test-html",
            "attempt_number": attempt_number,
            "status": status,
            "elapsed_seconds": 1.25,
            "item_count": if status == "success" { 3 } else { 0 },
            "failure_class": failure_class,
            "error_category": if failure_class.is_empty() { "" } else { "parse" },
            "failure_evidence": if failure_class.is_empty() {
                json!({})
            } else {
                json!({"url_host": format!("{route_id}.example.test"), "message": "頁面未完成渲染"})
            },
        })
    }

    #[test]
    fn browser_retry_policy_is_narrow_and_single_attempt() {
        let route = browser_route("vghtpe-html");
        assert!(should_retry_browser_route(
            &route,
            &ScraperError::BrowserRuntime("timeout".into()),
            1
        ));
        assert!(should_retry_browser_route(
            &route,
            &ScraperError::ParserRegression("not rendered".into()),
            1
        ));
        assert!(!should_retry_browser_route(
            &route,
            &ScraperError::ParserRegression("not rendered".into()),
            2
        ));
        assert!(!should_retry_browser_route(
            &route,
            &ScraperError::AccessBlocked("403".into()),
            1
        ));
        assert!(!should_retry_browser_route(
            &route,
            &ScraperError::Unknown("unknown".into()),
            1
        ));
        let mut html_route = route;
        html_route.kind = "html".into();
        assert!(!should_retry_browser_route(
            &html_route,
            &ScraperError::ParserRegression("changed".into()),
            1
        ));
    }

    #[test]
    fn dynamic_browser_parsers_wait_for_expected_dom() {
        for parser in ["sports-html", "vghtpe-html", "moenv-html", "moea-html"] {
            assert!(browser_page_script(parser).is_some(), "missing {parser}");
        }
        assert!(browser_page_script("standard").is_none());
    }

    #[test]
    fn retry_recovery_is_reported_as_unstable() {
        let result = SourceResult {
            source: "測試來源".into(),
            items: Vec::new(),
            error: None,
            attempts: vec![
                route_attempt("primary", 1, "failed", "parser_regression"),
                route_attempt("primary", 2, "success", ""),
            ],
            final_route: Some(json!({
                "route_id": "primary",
                "url": "https://primary.example.test/news",
                "url_host": "primary.example.test",
                "used_fallback": false,
                "coverage_reduced": false,
            })),
        };

        let diagnostic = source_diagnostic(&result);

        assert_eq!(diagnostic["status"], "success");
        assert_eq!(diagnostic["unstable"], true);
        assert_eq!(diagnostic["attempt_count"], 2);
        assert_eq!(diagnostic["last_failure_class"], "parser_regression");
    }

    #[test]
    fn exhausted_retry_remains_failed() {
        let result = SourceResult {
            source: "測試來源".into(),
            items: Vec::new(),
            error: Some(ScraperError::ParserRegression("still unavailable".into())),
            attempts: vec![
                route_attempt("primary", 1, "failed", "parser_regression"),
                route_attempt("primary", 2, "failed", "parser_regression"),
            ],
            final_route: None,
        };

        let diagnostic = source_diagnostic(&result);

        assert_eq!(diagnostic["status"], "failed");
        assert_eq!(diagnostic["unstable"], false);
        assert_eq!(diagnostic["failure_class"], "parser_regression");
        assert_eq!(diagnostic["route_attempt_count"], 2);
    }

    #[test]
    fn fallback_recovery_keeps_primary_failure_evidence() {
        let result = SourceResult {
            source: "測試來源".into(),
            items: Vec::new(),
            error: None,
            attempts: vec![
                route_attempt("primary", 1, "failed", "browser_runtime"),
                route_attempt("primary", 2, "failed", "browser_runtime"),
                route_attempt("fallback", 1, "success", ""),
            ],
            final_route: Some(json!({
                "route_id": "fallback",
                "url": "https://fallback.example.test/news",
                "url_host": "fallback.example.test",
                "used_fallback": true,
                "coverage_reduced": false,
            })),
        };

        let diagnostic = source_diagnostic(&result);

        assert_eq!(diagnostic["status"], "success");
        assert_eq!(diagnostic["unstable"], true);
        assert_eq!(diagnostic["final_route"]["used_fallback"], true);
        assert_eq!(diagnostic["last_failure_class"], "browser_runtime");
    }

    #[test]
    fn explicit_date_uses_its_calendar_week_even_on_monday() {
        let mut options = options();
        options.date = Some("2026-08-03".into());
        let range = resolve_date_range(&options).unwrap();
        assert_eq!(range.start.to_string(), "2026-08-03");
        assert_eq!(range.end.to_string(), "2026-08-09");
    }

    #[test]
    fn automatic_monday_uses_previous_complete_week() {
        let range = default_range_for_date(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap());
        assert_eq!(range.start.to_string(), "2026-07-27");
        assert_eq!(range.end.to_string(), "2026-08-02");
    }

    #[test]
    fn explicit_range_requires_ordered_dates() {
        let mut options = options();
        options.start_date = Some("2026-08-06".into());
        options.end_date = Some("2026-08-01".into());
        assert!(resolve_date_range(&options).is_err());
    }

    #[test]
    fn date_and_explicit_range_are_mutually_exclusive() {
        let mut options = options();
        options.date = Some("2026-08-06".into());
        options.start_date = Some("2026-08-01".into());
        options.end_date = Some("2026-08-06".into());
        assert!(resolve_date_range(&options).is_err());
    }

    #[test]
    fn rss_timestamp_is_converted_to_taipei_calendar_date() {
        assert_eq!(
            parse_date("Wed, 24 Jun 2026 16:00:00 GMT")
                .unwrap()
                .to_string(),
            "2026-06-25"
        );
        assert_eq!(
            parse_date("2026-06-24T16:30:00Z").unwrap().to_string(),
            "2026-06-25"
        );
    }

    #[test]
    fn naive_calendar_date_is_not_shifted() {
        assert_eq!(parse_date("2026-06-24").unwrap().to_string(), "2026-06-24");
    }

    #[test]
    fn excel_text_is_unicode_safe_and_respects_cell_limit() {
        let value = "政".repeat(EXCEL_CELL_CHAR_LIMIT + 10);
        let bounded = bounded_excel_text(&value);
        assert_eq!(bounded.chars().count(), EXCEL_CELL_CHAR_LIMIT);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn excel_agency_paths_match_affiliated_python_export() {
        assert_eq!(
            excel_agency_path("國土管理署", "國土管理署／都市基礎工程組"),
            ("內政部".into(), "國土管理署 / 都市基礎工程組".into())
        );
        assert_eq!(
            excel_agency_path("勞動基金運用局", "勞動基金運用局／企劃稽核組"),
            ("勞動部".into(), "勞動基金運用局 / 企劃稽核組".into())
        );
        assert_eq!(
            excel_agency_path("司法院", "司法院／臺灣臺北地方法院"),
            ("司法院".into(), "臺灣臺北地方法院".into())
        );
    }

    #[tokio::test]
    async fn cancelled_run_does_not_write_partial_artifacts() {
        let sandbox = tempfile::tempdir().unwrap();
        let output_dir = sandbox.path().join("output");
        let report_dir = sandbox.path().join("report");
        let mut options = options();
        options.sources = vec!["財政部".into()];
        options.output_dir = Some(output_dir.to_string_lossy().into_owned());
        options.report_dir = Some(report_dir.to_string_lossy().into_owned());
        let cancelled = Arc::new(AtomicBool::new(true));

        let error = run(&options, cancelled).await.unwrap_err();

        assert_eq!(error, "執行已取消");
        assert!(!output_dir.exists());
        assert!(!report_dir.exists());
    }

    #[test]
    fn report_dir_defaults_under_selected_output_dir() {
        let sandbox = tempfile::tempdir().unwrap();
        let output_dir = sandbox.path().join("selected-output");
        let mut options = options();
        options.output_dir = Some(output_dir.to_string_lossy().into_owned());

        let (_workbook_path, report_path) = write_outputs(
            &options,
            &[],
            0,
            DateRange {
                start: NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
            },
        )
        .unwrap();

        assert!(report_path.starts_with(output_dir.join("執行紀錄")));
    }
}
