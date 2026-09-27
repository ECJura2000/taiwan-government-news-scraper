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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

mod excel;
mod report;
use excel::*;

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
    detail: DetailDiagnostics,
}

#[derive(Debug, Default, Clone)]
struct DetailDiagnostics {
    attempted: usize,
    recovered: usize,
    issues: Vec<serde_json::Value>,
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
    if options.topics_json.is_some() && options.topics_policy.is_some() {
        return Err("主題設定不得同時指定檔案與內嵌設定".into());
    }
    let profile = if let Some(path) = &options.topics_json {
        crate::policy::read_profile(Path::new(path))?
    } else if let Some(profile) = &options.topics_policy {
        profile.clone()
    } else {
        crate::policy::Profile::embedded()
    };
    profile.require_enabled()?;
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
    let cache_root = PathBuf::from(options.output_dir.as_deref().unwrap_or(DEFAULT_OUTPUT_DIR));
    let client = HttpClient::new()
        .map_err(|error| error.to_string())?
        .with_cache_dir(cache_root.join(".http-cache"));
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
                        detail: DetailDiagnostics::default(),
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
            kind: "processing_items".into(),
            source: None,
            completed: Some(completed),
            total: Some(total),
            message: Some("正在整理並去除重複新聞".into()),
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
    let source_scraped_counts = source_counts.clone();
    let mut insecure_hosts: HashSet<String> =
        client.tls_fallback_hosts().await.into_iter().collect();
    for attempt in &source_attempts {
        if attempt["parser"] == "mnd-browser-tls-fallback" && attempt["status"] == "success" {
            if let Some(host) = attempt["url_host"].as_str() {
                insecure_hosts.insert(host.to_owned());
            }
        }
    }
    let mut insecure_ssl_hosts: Vec<String> = insecure_hosts.into_iter().collect();
    insecure_ssl_hosts.sort();
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
    let pre_policy_count = items.len();
    if let Some(progress) = &progress {
        progress(crate::ProgressEvent {
            kind: "ranking".into(),
            source: None,
            completed: Some(completed),
            total: Some(total),
            message: Some("正在計算政策相關性與排序".into()),
        });
    }
    let batch = crate::ranking::rank(&profile, &items);
    let classifications: Vec<_> = batch
        .results
        .iter()
        .filter(|r| r["hard_excluded"] != true)
        .cloned()
        .collect();
    items = items
        .into_iter()
        .zip(&batch.results)
        .filter(|(_, r)| r["hard_excluded"] != true)
        .map(|(item, _)| item)
        .collect();
    source_counts.values_mut().for_each(|v| *v = 0);
    for item in &items {
        *source_counts.entry(item.source.clone()).or_default() += 1;
    }
    if let Some(progress) = &progress {
        progress(crate::ProgressEvent {
            kind: "writing_outputs".into(),
            source: None,
            completed: Some(completed),
            total: Some(total),
            message: Some("正在產生 Excel 活頁簿".into()),
        });
    }
    let classified: Vec<_> = items
        .iter()
        .zip(&classifications)
        .map(|(item, classification)| ClassifiedNews {
            item,
            classification,
        })
        .collect();
    if classified.len() != items.len() {
        return Err("分類結果與新聞筆數不一致".into());
    }
    let paths = write_outputs(options, &classified, date_range, &profile)?;
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
    let report_dir = paths.1.parent().ok_or("JSON 報告路徑缺少資料夾")?;
    let mut relevance_policy = profile.summary();
    let anomalies = report::volume_anomalies(
        report_dir,
        date_range,
        relevance_policy["ruleset_hash"]
            .as_str()
            .unwrap_or_default(),
        &source_scraped_counts,
        &source_diagnostics,
    );
    if anomalies.iter().any(|value| value.starts_with("source_")) {
        alert_reasons.push("source_volume_regression");
    }
    if anomalies
        .iter()
        .any(|value| value.starts_with("detail_coverage_regression:"))
    {
        alert_reasons.push("detail_coverage_regression");
    }
    let status = if !failed_sources.is_empty() {
        "partial_failure"
    } else if !alert_reasons.is_empty() {
        "attention"
    } else {
        "success"
    };
    relevance_policy["pre_policy_count"] = json!(pre_policy_count);
    relevance_policy["excluded_news_count"] = json!(batch.excluded_count);
    relevance_policy["rule_counts"] = json!(batch.rule_counts);
    relevance_policy["topic_counts"] = json!(batch.topic_counts);
    let summary_count = items.iter().filter(|item| !item.summary.is_empty()).count();
    let full_text_count = items
        .iter()
        .filter(|item| !item.full_text.is_empty())
        .count();
    let mut source_full_text_counts: HashMap<String, usize> = HashMap::new();
    for item in &items {
        if !item.full_text.is_empty() {
            *source_full_text_counts
                .entry(item.source.clone())
                .or_default() += 1;
        }
    }
    let detail_attempted: usize = results.iter().map(|result| result.detail.attempted).sum();
    let detail_recovered: usize = results.iter().map(|result| result.detail.recovered).sum();
    let detail_failed_or_empty: usize = results
        .iter()
        .map(|result| result.detail.issues.len())
        .sum();
    let detail_zero_recovery_sources = report::zero_recovery_sources(&source_diagnostics);
    let mut detail_error_counts: HashMap<String, usize> = HashMap::new();
    for result in &results {
        for issue in &result.detail.issues {
            if let Some(reason) = issue["reason"].as_str() {
                *detail_error_counts.entry(reason.to_owned()).or_default() += 1;
            }
        }
    }
    let description_fallback_count = items
        .iter()
        .filter(|item| item.full_text.is_empty() && !item.summary.is_empty())
        .count();
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
    let full_text_coverage_rate = if items.is_empty() {
        0.0
    } else {
        ((full_text_count as f64 / items.len() as f64) * 10_000.0).round() / 10_000.0
    };
    let summary = crate::RunSummary {
        status: status.into(),
        news_count: items.len() as u64,
        failed_sources,
        anomalies,
        failure_class_counts: json!(failure_class_counts),
        source_health: json!({
            "healthy_count": source_diagnostics.iter().filter(|result| result.get("status").and_then(|value| value.as_str()) == Some("success") && result.get("unstable").and_then(|value| value.as_bool()) != Some(true)).count(),
            "unstable_count": source_diagnostics.iter().filter(|result| result.get("unstable").and_then(|value| value.as_bool()) == Some(true)).count(),
            "failed_count": results.iter().filter(|result| result.error.is_some()).count(),
            "fallback_source_count": source_diagnostics.iter().filter(|result| result.get("final_route").and_then(|route| route.get("used_fallback")).and_then(|value| value.as_bool()) == Some(true)).count(),
            "coverage_reduced_count": source_diagnostics.iter().filter(|result| result["final_route"]["coverage_reduced"] == true).count(),
            "ssl_fallback_host_count": insecure_ssl_hosts.len()
        }),
        quality: json!({
            "input_count": input_count,
            "pre_policy_count": pre_policy_count,
            "excluded_by_topic_rules_count": batch.excluded_count,
            "output_count": items.len(),
            "duplicate_count": duplicate_count,
            "invalid_count": invalid_count,
            "excluded_non_news_count": excluded_non_news_count,
            "source_counts": source_counts,
            "source_scraped_counts": source_scraped_counts,
            "summary_count": summary_count,
            "summary_coverage_rate": summary_coverage_rate,
            "full_text_count": full_text_count,
            "full_text_coverage_rate": full_text_coverage_rate,
            "source_full_text_counts": source_full_text_counts,
            "detail_fetch_attempted_count": detail_attempted,
            "detail_fetch_recovered_count": detail_recovered,
            "detail_fetch_failed_or_empty_count": detail_failed_or_empty,
            "detail_fetch_zero_recovery_sources": detail_zero_recovery_sources,
            "detail_fetch_error_counts": detail_error_counts,
            "date_source_counts": date_source_counts,
            "description_fallback_count": description_fallback_count,
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
        insecure_ssl_hosts,
        error: None,
    };
    if let Some(progress) = &progress {
        progress(crate::ProgressEvent {
            kind: "writing_report".into(),
            source: None,
            completed: Some(completed),
            total: Some(total),
            message: Some("正在寫入 JSON 執行報告".into()),
        });
    }
    report::write_json_report(&summary, &paths.1)?;
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
            detail: DetailDiagnostics::default(),
        };
    };
    let mut last_error = None;
    let mut attempts = Vec::new();
    let mut aggregated_items = Vec::new();
    let mut successful_routes = 0usize;
    let mut aggregate_final_route = None;
    let mut detail = DetailDiagnostics::default();
    'routes: for route in routes_for(definition) {
        let index = route.priority.saturating_sub(1) as usize;
        let url = &route.url;
        let host = url::Url::parse(url)
            .ok()
            .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_default();
        for attempt_number in 1..=2 {
            let started = Instant::now();
            let recent_nps_window =
                Local::now().with_timezone(&Taipei).date_naive() - chrono::Duration::days(180);
            let uses_recent_nps_prefix = source == "國家公園署"
                && route.parser == "nps-json"
                && date_range.end >= recent_nps_window;
            let fetched = if route.kind == "browser" {
                let page_script = browser_page_script(route.parser.as_str());
                if route.parser == "mnd-browser-tls-fallback" {
                    crate::browser::fetch_rendered_html_after_allow_invalid_certificates(
                        url,
                        page_script,
                    )
                    .await
                    .map_err(ScraperError::BrowserRuntime)
                } else {
                    crate::browser::fetch_rendered_html_after(url, page_script)
                        .await
                        .map_err(ScraperError::BrowserRuntime)
                }
            } else {
                if uses_recent_nps_prefix {
                    client.fetch_recent_json_array(url).await
                } else {
                    client.fetch_text(url).await
                }
            };
            let mut outcome = fetched.and_then(|body| adapters::parse_route(source, &route, &body));
            if uses_recent_nps_prefix
                && outcome
                    .as_ref()
                    .is_ok_and(|items| !items_cover_date_range(items, date_range))
            {
                outcome = client
                    .fetch_text(url)
                    .await
                    .and_then(|body| adapters::parse_route(source, &route, &body));
            }
            match outcome {
                Ok(items) => {
                    let parsed_item_count = items.len();
                    let filtered_items = filter_to_date_range(items, date_range);
                    let (filtered_items, route_detail) =
                        enrich_detail_full_text(client, filtered_items).await;
                    detail.attempted += route_detail.attempted;
                    detail.recovered += route_detail.recovered;
                    detail.issues.extend(route_detail.issues);
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
                        detail,
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
            detail,
            final_route: aggregate_final_route,
        };
    }
    SourceResult {
        source: source.to_owned(),
        items: Vec::new(),
        error: last_error.or_else(|| Some(ScraperError::SourceOutage("沒有可用來源入口".into()))),
        attempts,
        detail,
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
        "taicca-html" => Some(
            "(async () => { const deadline = Date.now() + 20000; while (Date.now() < deadline) { const item = document.querySelector('div.right-card-area > ul > li a.maintitle'); const date = document.querySelector('div.right-card-area > ul > li div.topbox div.date'); if (item && date && date.textContent.trim()) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        "mnd-browser-tls-fallback" => Some(
            "(async () => { const deadline = Date.now() + 20000; while (Date.now() < deadline) { const item = document.querySelector('div.news_list_box a.news_list'); if (item) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
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
        && route.priority > 1
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
        "detail_fetch": {
            "attempted": result.detail.attempted,
            "recovered": result.detail.recovered,
            "failed_or_empty": result.detail.issues.len(),
            "issues": result.detail.issues,
        },
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

fn items_cover_date_range(items: &[NewsItem], date_range: DateRange) -> bool {
    items
        .iter()
        .filter_map(|item| parse_date(&item.date))
        .min()
        .is_some_and(|oldest| oldest <= date_range.start)
}

async fn enrich_detail_full_text(
    client: &HttpClient,
    items: Vec<NewsItem>,
) -> (Vec<NewsItem>, DetailDiagnostics) {
    let outcomes: Vec<_> = stream::iter(items)
        .map(|mut item| {
            let client = client.clone();
            async move {
                if item.full_text.is_empty() && !item.link.is_empty() {
                    let issue = match client.fetch_detail_text(&item.link).await {
                        Ok(body) => {
                            let full_text = adapters::parse_detail_full_text(&item.source, &body);
                            if full_text.is_empty() {
                                Some(json!({"source": item.source, "url": item.link, "reason": "empty_detail_text"}))
                            } else {
                                item.summary = full_text.clone();
                                item.full_text = full_text;
                                None
                            }
                        }
                        Err(error) => Some(json!({"source": item.source, "url": item.link, "reason": error.failure_class().as_str()})),
                    };
                    (item, true, issue)
                } else {
                    (item, false, None)
                }
            }
        })
        .buffered(4)
        .collect()
        .await;
    let mut detail = DetailDiagnostics::default();
    let mut items = Vec::with_capacity(outcomes.len());
    for (item, attempted, issue) in outcomes {
        if attempted {
            detail.attempted += 1;
            if let Some(issue) = issue {
                detail.issues.push(issue);
            } else {
                detail.recovered += 1;
            }
        }
        items.push(item);
    }
    (items, detail)
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
            topics_json: None,
            topics_policy: None,
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
        assert!(!should_retry_browser_route(
            &route,
            &ScraperError::BrowserRuntime("timeout".into()),
            1
        ));
        assert!(!should_retry_browser_route(
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
        let mut fallback_route = route.clone();
        fallback_route.priority = 2;
        assert!(should_retry_browser_route(
            &fallback_route,
            &ScraperError::BrowserRuntime("timeout".into()),
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
            detail: DetailDiagnostics::default(),
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
            detail: DetailDiagnostics::default(),
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
            detail: DetailDiagnostics::default(),
        };

        let diagnostic = source_diagnostic(&result);

        assert_eq!(diagnostic["status"], "success");
        assert_eq!(diagnostic["unstable"], true);
        assert_eq!(diagnostic["final_route"]["used_fallback"], true);
        assert_eq!(diagnostic["last_failure_class"], "browser_runtime");
    }

    #[tokio::test]
    async fn detail_fetch_failure_keeps_item_and_records_reason() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/detail", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request);
            socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let item = NewsItem {
            source: "測試機關".into(),
            date: "2026-09-20".into(),
            department: String::new(),
            title: "測試新聞".into(),
            link: url.clone(),
            category: String::new(),
            summary: "列表摘要".into(),
            full_text: String::new(),
            date_source: "published".into(),
        };
        let (items, diagnostic) =
            enrich_detail_full_text(&HttpClient::new().unwrap(), vec![item]).await;
        server.join().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].summary, "列表摘要");
        assert_eq!(diagnostic.attempted, 1);
        assert_eq!(diagnostic.recovered, 0);
        assert_eq!(diagnostic.issues[0]["url"], url);
        assert_eq!(diagnostic.issues[0]["reason"], "parser_regression");
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
    fn recent_json_prefix_must_reach_the_requested_week() {
        let item = |date: &str| NewsItem {
            source: "國家公園署".into(),
            date: date.into(),
            department: "國家公園署".into(),
            title: "公園新聞".into(),
            link: "https://www.nps.gov.tw/ch/titlelist/parknews/1".into(),
            category: String::new(),
            summary: String::new(),
            full_text: String::new(),
            date_source: "published".into(),
        };
        let range = DateRange {
            start: NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 9, 20).unwrap(),
        };

        assert!(items_cover_date_range(
            &[item("2026-09-18"), item("2026-09-10")],
            range
        ));
        assert!(!items_cover_date_range(&[item("2026-09-18")], range));
    }

    #[test]
    fn excel_text_is_unicode_safe_and_respects_cell_limit() {
        let value = "政".repeat(EXCEL_CELL_CHAR_LIMIT + 10);
        let bounded = bounded_excel_text(&value);
        assert_eq!(bounded.chars().count(), EXCEL_CELL_CHAR_LIMIT);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn excel_source_label_keeps_the_clickable_url_visible() {
        let label = "數位發展部官網：https://moda.gov.tw/press/123";
        assert_eq!(
            extract_http_url(label),
            Some("https://moda.gov.tw/press/123")
        );
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

    #[test]
    fn selected_news_sort_prefers_rule_score_before_bm25() {
        let row = |title: &str, date: &str, bm25: &str| {
            let mut values = vec![String::new(); EXCEL_HEADERS.len()];
            values[1] = date.into();
            values[3] = title.into();
            values[15] = bm25.into();
            values[16] = format!("https://example.test/{title}");
            values
        };
        let mut rows = vec![
            (
                row("low-rule-high-bm25", "2026-09-12", "99"),
                40,
                "可能相關".into(),
            ),
            (
                row("high-rule-low-bm25", "2026-09-10", "1"),
                70,
                "可能相關".into(),
            ),
            (
                row("same-rule-newer", "2026-09-13", "10"),
                70,
                "可能相關".into(),
            ),
            (
                row("high-relevance", "2026-09-09", "0"),
                20,
                "高度相關".into(),
            ),
        ];

        sort_news_rows(&mut rows);

        let titles = rows
            .iter()
            .map(|(values, _, _)| values[3].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            vec![
                "high-relevance",
                "same-rule-newer",
                "high-rule-low-bm25",
                "low-rule-high-bm25",
            ]
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
    fn policy_workbook_has_copyable_sources_and_local_exclusions() {
        use std::io::Read;
        let profile=crate::policy::Profile::parse(r#"{"initiatives":[{"name":"甲","strong_keywords":["智慧醫療"],"exclude_keywords":[{"text":"徵才"}]},{"name":"乙","strong_keywords":["量子運算"]}]}"#).unwrap();
        let make = |title: &str| NewsItem {
            source: "國發會".into(),
            date: "2026-08-31".into(),
            title: title.into(),
            summary: String::new(),
            full_text: String::new(),
            link: "https://www.ndc.gov.tw/".into(),
            department: "國發會".into(),
            category: String::new(),
            date_source: "published".into(),
        };
        let items = vec![
            make("智慧醫療徵才：應完全移除"),
            make("智慧醫療徵才與量子運算：乙保留"),
            make("智慧醫療政策發布"),
        ];
        let batch = crate::ranking::rank(&profile, &items);
        let items: Vec<_> = items
            .into_iter()
            .zip(&batch.results)
            .filter(|(_, r)| r["hard_excluded"] != true)
            .map(|(n, _)| n)
            .collect();
        let results: Vec<_> = batch
            .results
            .into_iter()
            .filter(|r| r["hard_excluded"] != true)
            .collect();
        let temp = tempfile::tempdir().unwrap();
        let mut options = options();
        options.output_dir = Some(temp.path().to_string_lossy().into_owned());
        let classified: Vec<_> = items
            .iter()
            .zip(&results)
            .map(|(item, classification)| ClassifiedNews {
                item,
                classification,
            })
            .collect();
        let (path, _) = write_outputs(
            &options,
            &classified,
            DateRange {
                start: NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 9, 6).unwrap(),
            },
            &profile,
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut strings = String::new();
        archive
            .by_name("xl/sharedStrings.xml")
            .unwrap()
            .read_to_string(&mut strings)
            .unwrap();
        assert!(!strings.contains("應完全移除"));
        assert!(strings.contains("乙保留"));
        assert!(strings.contains("開啟原文"));
        let source_label = "國發會官網：https://www.ndc.gov.tw/";
        let label_position = strings
            .find(source_label)
            .expect("missing source link label");
        let label_index = strings[..label_position]
            .matches("<si>")
            .count()
            .checked_sub(1)
            .expect("source link label must be inside a shared string");
        let pattern = Regex::new(r#"<hyperlink ref="([A-Z]+)[0-9]+""#).unwrap();
        let labeled_cell_pattern = Regex::new(&format!(
            r#"<c r="(Q[0-9]+)"[^>]*><v>{label_index}</v></c>"#
        ))
        .unwrap();
        for name in [
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/worksheets/sheet3.xml",
            "xl/worksheets/sheet4.xml",
        ] {
            let mut xml = String::new();
            archive
                .by_name(name)
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            assert!(pattern.is_match(&xml), "missing explicit original link");
            for cap in pattern.captures_iter(&xml) {
                assert_eq!(&cap[1], "Q");
            }
            let labeled_cell = labeled_cell_pattern
                .captures(&xml)
                .expect("source label is not written in the hyperlink column");
            assert!(xml.contains(&format!(r#"<hyperlink ref="{}" "#, &labeled_cell[1])));
            assert!(xml.contains("2026/08/31"));
            assert!(xml.contains("115/08/31"));
            assert!(!xml.contains("民國115"));
        }
        assert_eq!(excel_date("2026-08-31"), "2026/08/31");
        assert_eq!(roc_date("2026-08-31").unwrap(), "115/08/31");
        if let Some(dir) = std::env::var_os("NEWS_SCRAPER_QA_OUTPUT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(path, PathBuf::from(dir).join("stage1-verification.xlsx")).unwrap();
        }
    }

    #[test]
    fn excel_overview_label_preserves_seventeen_columns_and_formal_topics() {
        let item = NewsItem {
            source: "國科會".into(),
            date: "2026-08-26".into(),
            title: "醫療與農業 AI 應用".into(),
            summary: String::new(),
            full_text: String::new(),
            link: "https://www.nstc.gov.tw/".into(),
            department: String::new(),
            category: String::new(),
            date_source: "published".into(),
        };
        let result = json!({
            "topics": ["全民智慧生活圈", "百工百業智慧應用"],
            "score": 60,
            "relevance": "可能相關",
            "reasons": ["政策間接關聯"]
        });
        let (row, _, _) = excel_row(&item, &result);
        assert_eq!(row.len(), EXCEL_HEADERS.len());
        assert_eq!(row[7], "綜整性、全民智慧生活圈、百工百業智慧應用");
        assert_eq!(row[11], "政策間接關聯");
        assert_eq!(result["topics"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn topic_sheet_names_are_unique_and_valid() {
        let mut used = std::collections::BTreeSet::new();
        let a = unique_sheet_name("[]:*?/\\abcdefghijklmnopqrstuvwxyzabcdef", &mut used);
        let b = unique_sheet_name("[]:*?/\\abcdefghijklmnopqrstuvwxyzabcdef", &mut used);
        assert_ne!(a, b);
        assert!(a.encode_utf16().count() <= 31);
        assert!(!a.contains('/'));
        assert_eq!(unique_sheet_name("History", &mut used), "History (1)");
        assert!(extract_http_url("https://").is_none());
        assert!(extract_http_url("javascript:alert(1)").is_none());
        assert_eq!(
            extract_http_url("https://www.ndc.gov.tw/"),
            Some("https://www.ndc.gov.tw/")
        );
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
            DateRange {
                start: NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
            },
            &crate::policy::Profile::embedded(),
        )
        .unwrap();

        assert!(report_path.starts_with(output_dir.join("執行紀錄")));
    }
}
