use super::{DateRange, ProgressCallback};
use crate::scraper::adapters;
use crate::scraper::catalog::{find_source, routes_for, SourceDefinition, SourceRoute};
use crate::scraper::http::HttpClient;
use crate::scraper::transport::TransportPolicy;
use crate::scraper::{NewsItem, ScraperError};
use chrono::{Local, NaiveDate};
use chrono_tz::Asia::Taipei;
use futures::stream::{self, StreamExt};
use regex::Regex;
use serde_json::json;
use std::time::Instant;

const RECENT_JSON_PREFIX_END: usize = 262_143;

#[derive(Debug)]
pub(super) struct SourceResult {
    pub(super) source: String,
    pub(super) items: Vec<NewsItem>,
    pub(super) error: Option<ScraperError>,
    pub(super) attempts: Vec<serde_json::Value>,
    pub(super) final_route: Option<serde_json::Value>,
    pub(super) detail: DetailDiagnostics,
}

#[derive(Debug, Default, Clone)]
pub(super) struct DetailDiagnostics {
    pub(super) attempted: usize,
    pub(super) recovered: usize,
    pub(super) issues: Vec<serde_json::Value>,
    pub(super) discovery_items: Vec<NewsItem>,
    pub(super) item_records: Vec<serde_json::Value>,
    pub(super) list_items: usize,
    pub(super) date_filtered: usize,
    pub(super) discovery_seconds: f64,
    pub(super) date_filter_seconds: f64,
    pub(super) fetch_seconds: f64,
    pub(super) browser_fallbacks: Vec<serde_json::Value>,
}

pub(super) async fn fetch_source(
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
    fetch_source_definition(client, definition, source, date_range, progress, total).await
}

async fn fetch_source_definition(
    client: &HttpClient,
    definition: &SourceDefinition,
    source: &str,
    date_range: DateRange,
    progress: Option<&ProgressCallback>,
    total: u32,
) -> SourceResult {
    let mut last_error = None;
    let mut attempts = Vec::new();
    let mut aggregated_items = Vec::new();
    let mut successful_routes = 0usize;
    let mut aggregate_final_route = None;
    let mut detail = DetailDiagnostics::default();
    'routes: for route in routes_for(definition) {
        let policy = TransportPolicy::for_route(definition, &route);
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
                    if source == "國防部"
                        && route.id == "official-browser-tls-fallback"
                        && policy.allows_tls_fallback_for(url)
                    {
                        crate::browser::fetch_rendered_html_after_allow_invalid_certificates(
                            url,
                            page_script,
                        )
                        .await
                        .map_err(classify_browser_error)
                    } else {
                        Err(ScraperError::AccessBlocked(
                            "undeclared browser TLS fallback route".into(),
                        ))
                    }
                } else {
                    crate::browser::fetch_rendered_html_after(url, page_script)
                        .await
                        .map_err(classify_browser_error)
                }
            } else {
                if uses_recent_nps_prefix {
                    client
                        .fetch_range_prefix(url, &policy, RECENT_JSON_PREFIX_END)
                        .await
                        .and_then(|bytes| complete_json_array_prefix(&bytes))
                } else {
                    client.fetch_text(url, &policy).await
                }
            };
            let mut outcome = fetched.and_then(|body| adapters::parse_route(source, &route, &body));
            // An official JSON snapshot can lag behind its corresponding list.
            // Never silently accept an out-of-date snapshot as an empty week.
            if route.parser == "thb-json"
                && outcome
                    .as_ref()
                    .is_ok_and(|items| snapshot_predates_week(items, date_range))
            {
                outcome = Err(ScraperError::SourceOutage(
                    "official JSON snapshot predates the requested week; try the live list".into(),
                ));
            }
            if uses_recent_nps_prefix
                && outcome
                    .as_ref()
                    .is_ok_and(|items| !items_cover_date_range(items, date_range))
            {
                outcome = client
                    .fetch_text(url, &policy)
                    .await
                    .and_then(|body| adapters::parse_route(source, &route, &body));
            }
            match outcome {
                Ok(items) => {
                    detail.discovery_seconds += started.elapsed().as_secs_f64();
                    let parsed_item_count = items.len();
                    detail.list_items += parsed_item_count;
                    let filtering_started = Instant::now();
                    let filtered_items = filter_to_date_range(items, date_range);
                    detail.date_filter_seconds += filtering_started.elapsed().as_secs_f64();
                    detail.date_filtered += filtered_items.len();
                    detail.item_records.extend(filtered_items.iter().map(|item| json!({
                        "source":item.source, "date":item.date, "title":item.title, "link":item.link,
                        "list_summary":item.summary, "route":{"route_id":route.id, "url":url,
                            "parser":route.parser, "route_kind":route.kind}
                    })));
                    detail.discovery_items.extend(filtered_items.clone());
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
                    detail.discovery_seconds += started.elapsed().as_secs_f64();
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

pub(super) fn browser_page_script(parser: &str) -> Option<&'static str> {
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
        "hakka-html" => Some(
            "(async () => { const deadline = Date.now() + 15000; while (Date.now() < deadline) { if (document.querySelector('div.list ul li p.subject')) return true; await new Promise(resolve => setTimeout(resolve, 250)); } return false; })()",
        ),
        _ => None,
    }
}

fn classify_browser_error(error: String) -> ScraperError {
    if error.starts_with("access_blocked:") || error.contains("Incapsula 驗證超過") {
        ScraperError::AccessBlocked(error)
    } else {
        ScraperError::BrowserRuntime(error)
    }
}

fn snapshot_predates_week(items: &[NewsItem], range: DateRange) -> bool {
    items
        .iter()
        .filter_map(|item| parse_date(&item.date))
        .max()
        .is_none_or(|latest| latest < range.start)
}

pub(super) fn should_retry_browser_route(
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

pub(super) fn source_diagnostic(result: &SourceResult) -> serde_json::Value {
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
            "browser_fallbacks": result.detail.browser_fallbacks,
        },
        "discovery": {
            "list_items":result.detail.list_items, "date_filtered":result.detail.date_filtered,
            "discovery_seconds":result.detail.discovery_seconds,
            "date_filter_seconds":result.detail.date_filter_seconds,
            "detail_fetch_seconds":result.detail.fetch_seconds,
            "items":result.detail.item_records,
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

pub(super) fn items_cover_date_range(items: &[NewsItem], date_range: DateRange) -> bool {
    items
        .iter()
        .filter_map(|item| parse_date(&item.date))
        .min()
        .is_some_and(|oldest| oldest <= date_range.start)
}

pub(super) async fn enrich_detail_full_text(
    client: &HttpClient,
    items: Vec<NewsItem>,
) -> (Vec<NewsItem>, DetailDiagnostics) {
    let started = Instant::now();
    let outcomes: Vec<_> = stream::iter(items)
        .map(|mut item| {
            let client = client.clone();
            async move {
                if item.full_text.is_empty() && !item.link.is_empty() {
                    let policy = find_source(&item.source)
                        .map_or_else(TransportPolicy::detail_default, |source| {
                            TransportPolicy::for_detail(source, &item.link)
                        });
                    let mut fetched = client.fetch_detail_text(&item.link, &policy).await;
                    let mut fallback = None;
                    if policy.browser_fallback && !fetched.as_ref().is_ok_and(|body| !adapters::parse_detail_full_text(&item.source, body).is_empty()) {
                        let initial_reason = fetched.as_ref().err().map_or("empty_detail_text", |error| error.failure_class().as_str());
                        let rendered = crate::browser::fetch_rendered_html_after_with_timeout(&item.link, None, policy.timeout)
                            .await.map_err(classify_browser_error);
                        fallback = Some(json!({"source":item.source,"url":item.link,"http_reason":initial_reason,
                            "browser_status":if rendered.is_ok(){"success"}else{"failed"},
                            "browser_failure_class":rendered.as_ref().err().map(|error|error.failure_class().as_str())}));
                        fetched = rendered;
                    }
                    let issue = match fetched {
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
                    (item, true, issue, fallback)
                } else {
                    (item, false, None, None)
                }
            }
        })
        .buffered(4)
        .collect()
        .await;
    let mut detail = DetailDiagnostics::default();
    let mut items = Vec::with_capacity(outcomes.len());
    for (item, attempted, issue, fallback) in outcomes {
        if let Some(fallback) = fallback {
            detail.browser_fallbacks.push(fallback);
        }
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
    detail.fetch_seconds = started.elapsed().as_secs_f64();
    (items, detail)
}

/// Discovery remains independently usable; orchestration chooses whether to enrich.
pub(super) async fn enrich_source(
    client: &HttpClient,
    result: &mut SourceResult,
    mode: crate::ContentMode,
) {
    if mode == crate::ContentMode::Full {
        let (items, diagnostics) =
            enrich_detail_full_text(client, std::mem::take(&mut result.items)).await;
        result.items = items;
        result.detail.attempted = diagnostics.attempted;
        result.detail.recovered = diagnostics.recovered;
        result.detail.issues = diagnostics.issues;
        result.detail.fetch_seconds = diagnostics.fetch_seconds;
        result.detail.browser_fallbacks = diagnostics.browser_fallbacks;
    } else {
        for item in &mut result.items {
            item.full_text.clear();
        }
    }
    result.detail.item_records = result
        .items
        .iter()
        .zip(&result.detail.discovery_items)
        .enumerate()
        .map(|(index, (item, discovery))| {
            let status = if mode == crate::ContentMode::Summary {
                "not_needed"
            } else if !item.full_text.is_empty() {
                "fetched"
            } else if item.link.is_empty() {
                "not_needed"
            } else {
                "failed"
            };
            json!({"source": item.source, "date": item.date, "title": item.title,
            "link": item.link, "list_summary": discovery.summary, "detail_status": status,
            "route": result.detail.item_records.get(index).and_then(|record| record.get("route"))})
        })
        .collect();
}

pub(super) fn parse_date(value: &str) -> Option<NaiveDate> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_or_empty_official_snapshot_must_not_end_fallbacks() {
        let range = DateRange {
            start: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 10, 4).unwrap(),
        };
        assert!(snapshot_predates_week(&[], range));
        for (date, stale) in [
            ("2026-08-19", true),
            ("2026-09-28", false),
            ("2026-10-05", false),
        ] {
            let body = format!("<rss><channel><item><title>測試新聞</title><link>https://example.test/1</link><pubDate>{date}</pubDate></item></channel></rss>");
            let items = crate::scraper::rss::parse_feed("測試", &body).unwrap();
            assert_eq!(snapshot_predates_week(&items, range), stale);
        }
    }

    #[test]
    fn browser_access_denial_is_not_retryable_browser_runtime() {
        let route = routes_for(find_source("公路局").unwrap()).pop().unwrap();
        let error = classify_browser_error("access_blocked: rejected".into());
        assert!(matches!(error, ScraperError::AccessBlocked(_)));
        assert!(!should_retry_browser_route(&route, &error, 1));
        assert!(matches!(
            classify_browser_error("CDP websocket disconnected".into()),
            ScraperError::BrowserRuntime(_)
        ));
    }
    use crate::scraper::transport::PolicyOverrides;

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

    #[tokio::test]
    async fn fixed_route_fixture_preserves_attempts_and_zero_news_success() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (status, body) in [
                ("503 Service Unavailable", ""),
                ("200 OK", "<rss><channel></channel></rss>"),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request);
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let route = |id: &str, suffix: &str, kind: &str, priority| SourceRoute {
            id: id.into(),
            url: format!("{base}/{suffix}"),
            kind: kind.into(),
            parser: "standard".into(),
            priority,
            official: true,
            coverage_reduced: false,
            selectors: None,
            transport: Some(PolicyOverrides {
                retry_attempts: Some(1),
                ..PolicyOverrides::default()
            }),
        };
        let definition = SourceDefinition {
            name: "固定來源".into(),
            urls: vec![format!("{base}/first"), format!("{base}/second")],
            parent_ministry: None,
            aggregate_routes: false,
            routes: vec![
                route("primary", "first", "html", 1),
                route("fallback", "second", "rss", 2),
            ],
            transport: None,
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 21).unwrap();
        let result = fetch_source_definition(
            &HttpClient::new().unwrap(),
            &definition,
            &definition.name,
            DateRange {
                start: date,
                end: date,
            },
            None,
            1,
        )
        .await;
        server.join().unwrap();
        assert!(result.error.is_none());
        assert!(result.items.is_empty());
        assert_eq!(result.attempts.len(), 2);
        assert_eq!(result.attempts[0]["route_id"], "primary");
        assert_eq!(result.attempts[0]["failure_class"], "source_outage");
        assert_eq!(result.attempts[1]["route_id"], "fallback");
        assert_eq!(result.attempts[1]["status"], "success");
        let diagnostic = source_diagnostic(&result);
        assert_eq!(diagnostic["status"], "success");
        assert_eq!(diagnostic["unstable"], true);
        assert_eq!(diagnostic["item_count"], 0);
        assert_eq!(diagnostic["final_route"]["used_fallback"], true);
    }
}
