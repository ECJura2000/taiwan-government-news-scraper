use super::{DateRange, ProgressCallback};
use crate::scraper::adapters;
use crate::scraper::catalog::{find_source, routes_for, SourceDefinition, SourceRoute};
use crate::scraper::http::HttpClient;
use crate::scraper::transport::TransportPolicy;
use crate::scraper::{NewsItem, ScraperError};
use chrono::NaiveDate;
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
    pub(super) filter_decisions: Vec<serde_json::Value>,
    pub(super) overlap_decisions: Vec<serde_json::Value>,
    pub(super) discovery_seconds: f64,
    pub(super) date_filter_seconds: f64,
    pub(super) fetch_seconds: f64,
    pub(super) browser_fallbacks: Vec<serde_json::Value>,
    pub(super) requests: Vec<serde_json::Value>,
}

pub(super) async fn fetch_source(
    client: &HttpClient,
    source: &str,
    date_range: DateRange,
    progress: Option<&ProgressCallback>,
    total: u32,
    today: NaiveDate,
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
    fetch_source_definition(
        client, definition, source, date_range, progress, total, today,
    )
    .await
}

async fn fetch_source_definition(
    client: &HttpClient,
    definition: &SourceDefinition,
    source: &str,
    date_range: DateRange,
    progress: Option<&ProgressCallback>,
    total: u32,
    today: NaiveDate,
) -> SourceResult {
    let mut last_error = None;
    let mut attempts = Vec::new();
    let mut aggregated_items = Vec::new();
    let mut successful_routes = 0usize;
    let mut coverage_pending = false;
    let mut aggregate_final_route = None;
    let mut detail = DetailDiagnostics::default();
    let routes = routes_for(definition);
    'routes: for (route_position, route) in routes.iter().enumerate() {
        let policy = TransportPolicy::for_route(definition, route);
        let index = route.priority.saturating_sub(1) as usize;
        let url = &route.url;
        let host = url::Url::parse(url)
            .ok()
            .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_default();
        let page_script = route_page_script(route);
        for attempt_number in 1..=2 {
            let started = Instant::now();
            let recent_nps_window = today - chrono::Duration::days(180);
            let uses_recent_nps_prefix = source == "國家公園署"
                && route.parser == "nps-json"
                && date_range.end >= recent_nps_window;
            let fetched = if route.kind == "browser" {
                if route.parser == "mnd-browser-tls-fallback" {
                    if source == "國防部"
                        && route.id == "official-browser-tls-fallback"
                        && policy.allows_tls_fallback_for(url)
                    {
                        crate::browser::fetch_rendered_html_after_allow_invalid_certificates(
                            url,
                            page_script.as_deref(),
                        )
                        .await
                        .map_err(classify_browser_error)
                    } else {
                        Err(ScraperError::AccessBlocked(
                            "undeclared browser TLS fallback route".into(),
                        ))
                    }
                } else {
                    crate::browser::fetch_rendered_html_after_with_transport(
                        url,
                        page_script.as_deref(),
                        policy.browser_page_timeout,
                        policy.browser_native_user_agent,
                        policy.browser_entry_url.as_deref(),
                    )
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
            let mut outcome =
                fetched.and_then(|body| adapters::parse_route_records(source, route, &body));
            // An official JSON snapshot can lag behind its corresponding list.
            // Never silently accept an out-of-date snapshot as an empty week.
            if matches!(route.parser.as_str(), "thb-json" | "moenv-open-data-json")
                && outcome.as_ref().is_ok_and(|records| {
                    snapshot_predates_week(
                        &records.iter().map(|r| r.item.clone()).collect::<Vec<_>>(),
                        date_range,
                    )
                })
            {
                outcome = Err(ScraperError::SourceOutage(
                    "official JSON snapshot predates the requested week; try the live list".into(),
                ));
            }
            if route.parser == "moenv-open-data-json"
                && outcome.as_ref().is_ok_and(|records| {
                    records
                        .iter()
                        .filter_map(|record| parse_date(&record.item.date))
                        .min()
                        .is_none_or(|oldest| oldest > date_range.start)
                })
            {
                outcome = Err(ScraperError::SourceOutage(
                    "official news datastore window does not reach the requested week".into(),
                ));
            }
            if uses_recent_nps_prefix
                && outcome.as_ref().is_ok_and(|records| {
                    !items_cover_date_range(
                        &records.iter().map(|r| r.item.clone()).collect::<Vec<_>>(),
                        date_range,
                    )
                })
            {
                outcome = client
                    .fetch_text(url, &policy)
                    .await
                    .and_then(|body| adapters::parse_route_records(source, route, &body));
            }
            let mut pagination_incomplete = false;
            let first_page = route
                .pagination
                .as_ref()
                .map(|pagination| pagination.first_page);
            let mut origins = outcome.as_ref().map_or_else(
                |_| Vec::new(),
                |records| vec![record_origin(route, url, first_page); records.len()],
            );
            if let Some(pagination) = &route.pagination {
                if let Ok(records) = &mut outcome {
                    let mut page = pagination.first_page;
                    let mut last_page = records.clone();
                    let mut last_page_url = url.to_owned();
                    let mut visited = std::collections::HashSet::new();
                    let mut frontier_articles = std::collections::HashSet::new();
                    visited.insert(
                        last_page
                            .iter()
                            .map(|r| r.item.link.clone())
                            .collect::<Vec<_>>(),
                    );
                    loop {
                        // Pinned announcements can appear at either end of the list.
                        // A single old row cannot prove the requested week is exhausted.
                        let latest = last_page
                            .iter()
                            .filter(|record| frontier_articles.insert(record.item.link.clone()))
                            .filter_map(|r| parse_date(&r.item.date))
                            .max();
                        if last_page.is_empty()
                            || latest.is_some_and(|date| date < date_range.start)
                        {
                            break;
                        }
                        if page - pagination.first_page + 1 >= pagination.max_pages {
                            let error = ScraperError::SourceOutage(
                                "official list page limit reached before requested week".into(),
                            );
                            let mut evidence = attempt_json(
                                source,
                                route,
                                &host,
                                Instant::now(),
                                attempt_number,
                                &error,
                            );
                            evidence["url"] = json!(last_page_url);
                            evidence["page"] = json!(page);
                            evidence["failure_evidence"]["stage"] = json!("pagination_limit");
                            evidence["failure_evidence"]["max_pages"] = json!(pagination.max_pages);
                            attempts.push(evidence);
                            last_error = Some(error);
                            pagination_incomplete = true;
                            break;
                        }
                        page += 1;
                        let mut page_url = url::Url::parse(url).expect("validated route URL");
                        let pairs: Vec<_> = page_url
                            .query_pairs()
                            .filter(|(key, _)| key != pagination.parameter.as_str())
                            .map(|(k, v)| (k.into_owned(), v.into_owned()))
                            .collect();
                        page_url.set_query(None);
                        page_url
                            .query_pairs_mut()
                            .extend_pairs(pairs)
                            .append_pair(&pagination.parameter, &page.to_string());
                        let started_page = Instant::now();
                        let next = if route.kind == "browser" {
                            crate::browser::fetch_rendered_html_after_with_transport(
                                page_url.as_str(),
                                page_script.as_deref(),
                                policy.browser_page_timeout,
                                policy.browser_native_user_agent,
                                policy.browser_entry_url.as_deref(),
                            )
                            .await
                            .map_err(classify_browser_error)
                        } else {
                            client.fetch_text(page_url.as_str(), &policy).await
                        };
                        match next
                            .and_then(|body| adapters::parse_route_records(source, route, &body))
                        {
                            Ok(next_records) => {
                                let key = next_records
                                    .iter()
                                    .map(|r| r.item.link.clone())
                                    .collect::<Vec<_>>();
                                if !next_records.is_empty() && !visited.insert(key) {
                                    let error = ScraperError::ParserRegression(
                                        "official pagination repeated the same article page".into(),
                                    );
                                    let mut evidence = attempt_json(
                                        source,
                                        route,
                                        &host,
                                        started_page,
                                        attempt_number,
                                        &error,
                                    );
                                    evidence["url"] = json!(page_url);
                                    evidence["page"] = json!(page);
                                    evidence["failure_evidence"]["stage"] =
                                        json!("pagination_repeated_page");
                                    attempts.push(evidence);
                                    last_error = Some(error);
                                    pagination_incomplete = true;
                                    break;
                                }
                                attempts.push(json!({"source":source,"route_id":route.id,"url":page_url,"route_kind":route.kind,"parser":route.parser,"attempt_number":1,"status":"success","page":page,"item_count":next_records.len(),"elapsed_seconds":elapsed_seconds(started_page),"failure_class":"","error_category":""}));
                                last_page_url = page_url.to_string();
                                last_page = next_records.clone();
                                origins.extend(vec![
                                    record_origin(
                                        route,
                                        page_url.as_str(),
                                        Some(page)
                                    );
                                    next_records.len()
                                ]);
                                records.extend(next_records);
                            }
                            Err(error) => {
                                attempts.push(json!({"source":source,"route_id":route.id,"url":page_url,"route_kind":route.kind,"parser":route.parser,"attempt_number":1,"status":"failed","page":page,"failure_class":error.failure_class(),"error_category":error.error_category(),"elapsed_seconds":elapsed_seconds(started_page),"failure_evidence":{"message":error.to_string()}}));
                                last_error = Some(error);
                                pagination_incomplete = true;
                                break;
                            }
                        }
                    }
                }
            }
            match outcome {
                Ok(records) => {
                    detail.discovery_seconds += started.elapsed().as_secs_f64();
                    let parsed_item_count = records.len();
                    detail.list_items += parsed_item_count;
                    let mut accepted = Vec::new();
                    let mut filtered_out = 0;
                    for (record, origin) in records.into_iter().zip(origins) {
                        let (records, mut decisions) =
                            crate::scraper::filter::apply_records(definition, route, vec![record]);
                        filtered_out += decisions.len();
                        for decision in &mut decisions {
                            decision["origin"] = origin.clone();
                        }
                        detail.filter_decisions.extend(decisions);
                        accepted.extend(records.into_iter().map(|record| (record, origin.clone())));
                    }
                    let items = accepted
                        .iter()
                        .map(|(record, _)| record.item.clone())
                        .collect::<Vec<_>>();
                    let is_feed = route.kind == "rss"
                        || route.parser.contains("rss")
                        || route.parser == "thb-xml"
                        || (route.parser == "thb-json" && route.coverage_reduced);
                    let feed_window_covered = !is_feed
                        || (!route.coverage_reduced
                            && items
                                .iter()
                                .filter_map(|item| parse_date(&item.date))
                                .min()
                                .is_some_and(|oldest| oldest < date_range.start)
                            && !snapshot_predates_week(&items, date_range));
                    let needs_feed_fallback =
                        is_feed && route_position + 1 < routes.len() && !feed_window_covered;
                    let filtering_started = Instant::now();
                    for (mut record, origin) in accepted {
                        if let Some(item) =
                            filter_to_date_range(vec![record.item.clone()], date_range).pop()
                        {
                            record.item = item;
                            detail.date_filtered += 1;
                            merge_discovered_record(
                                &mut aggregated_items,
                                &mut detail,
                                record,
                                origin,
                            );
                        }
                    }
                    detail.date_filter_seconds += filtering_started.elapsed().as_secs_f64();
                    attempts.push(json!({
                        "source": source,
                        "route_id": route.id,
                        "url": url,
                        "url_host": host,
                        "route_kind": route.kind.as_str(),
                        "parser": route.parser.as_str(),
                        "attempt_number": attempt_number,
                        "status": "success",
                        "tls_intermediate_profile": (cfg!(target_os = "linux") && route.kind != "browser" && policy.intermediate_certificate_for(url).is_some()).then_some("twca-secure2023"),
                        "elapsed_seconds": elapsed_seconds(started),
                        "item_count": parsed_item_count,
                        "content_filtered_out": filtered_out,
                        "requested_window_covered": feed_window_covered && !pagination_incomplete,
                        "continuing_for_coverage": needs_feed_fallback,
                        "failure_class": "",
                        "error_category": "",
                        "failure_evidence": {},
                    }));
                    coverage_pending = pagination_incomplete || needs_feed_fallback;
                    if coverage_pending && !needs_feed_fallback && route.pagination.is_some() {
                        successful_routes += 1;
                        aggregate_final_route = Some(
                            json!({"route_id":route.id,"url":url,"url_host":host,"used_fallback":index>0,"coverage_reduced":true,"aggregated":true}),
                        );
                        continue 'routes;
                    }
                    if definition.aggregate_routes || needs_feed_fallback {
                        successful_routes += 1;
                        aggregate_final_route = Some(json!({
                            "route_id": route.id,
                            "url": url,
                            "url_host": host,
                            "used_fallback": false,
                            "coverage_reduced": route.coverage_reduced || needs_feed_fallback,
                            "aggregated": true,
                        }));
                        continue 'routes;
                    }
                    return SourceResult {
                        source: source.to_owned(),
                        items: aggregated_items,
                        error: None,
                        final_route: Some(json!({
                            "route_id": route.id,
                            "url": url,
                            "url_host": host,
                            "used_fallback": index > 0,
                            "coverage_reduced": route.coverage_reduced || !feed_window_covered,
                        })),
                        attempts,
                        detail,
                    };
                }
                Err(error) => {
                    detail.discovery_seconds += started.elapsed().as_secs_f64();
                    let should_retry = should_retry_browser_route(route, &error, attempt_number);
                    attempts.push(attempt_json(
                        source,
                        route,
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
            error: if coverage_pending { last_error } else { None },
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

fn record_origin(route: &SourceRoute, url: &str, page: Option<u32>) -> serde_json::Value {
    json!({"route_id":route.id,"url":url,"parser":route.parser,"route_kind":route.kind,"page":page})
}

/// Match one occurrence per route/page. Multiplicity within any individual page
/// remains visible to the downstream quality check, even when pages overlap.
fn merge_discovered_record(
    items: &mut Vec<NewsItem>,
    detail: &mut DetailDiagnostics,
    record: crate::scraper::rss::FeedRecord,
    origin: serde_json::Value,
) {
    let item = record.item;
    let evidence = json!({"route":origin,"publisher":record.publisher,"categories":record.categories,
        "source":item.source,"date":item.date,"date_source":item.date_source,"title":item.title,"link":item.link,
        "list_summary":item.summary,"full_text_length":item.full_text.chars().count()});
    let matched = items.iter().enumerate().position(|(index, existing)| {
        existing.source == item.source
            && existing.date == item.date
            && existing.title == item.title
            && existing.link == item.link
            && !item.link.is_empty()
            && detail.item_records[index]["origins"]
                .as_array()
                .is_some_and(|origins| !origins.iter().any(|previous| previous["route"] == origin))
    });
    if let Some(index) = matched {
        let reason = if detail.item_records[index]["route"]["route_id"] == origin["route_id"] {
            "cross_page_overlap"
        } else {
            "cross_route_overlap"
        };
        detail
            .overlap_decisions
            .push(json!({"reason":reason,"retained_index":index,
            "key":{"source":item.source,"date":item.date,"title":item.title,"link":item.link},
            "original":evidence}));
        if item.full_text.len() > items[index].full_text.len() {
            items[index].full_text = item.full_text;
        }
        if item.summary.len() > items[index].summary.len() {
            items[index].summary = item.summary;
        }
        detail.discovery_items[index] = items[index].clone();
        detail.item_records[index]["list_summary"] = json!(items[index].summary);
        detail.item_records[index]["origins"]
            .as_array_mut()
            .expect("origin evidence array")
            .push(evidence);
    } else {
        detail.item_records.push(
            json!({"source":item.source,"date":item.date,"title":item.title,
            "link":item.link,"list_summary":item.summary,"route":origin,
            "publisher":record.publisher,"categories":record.categories,"origins":[evidence]}),
        );
        detail.discovery_items.push(item.clone());
        items.push(item);
    }
}

fn route_page_script(route: &SourceRoute) -> Option<String> {
    route.wait_for_selector.as_ref().map(|selector|format!(
        "(async()=>{{for(let i=0;i<20;i++){{if(document.querySelector({}))return;await new Promise(r=>setTimeout(r,1000));}}throw new Error('official news selector not ready');}})()",
        serde_json::to_string(selector).expect("selector JSON")))
        .or_else(||browser_page_script(route.parser.as_str()).map(str::to_owned))
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
    } else if error.starts_with("browser_navigation_tls:") {
        ScraperError::TlsCertificate(error)
    } else if error.starts_with("browser_navigation_network:") {
        ScraperError::RunnerNetwork(error)
    } else if error.starts_with("browser_navigation_outage:") {
        ScraperError::SourceOutage(error)
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
        "failure_class": result.error.as_ref().map(|error|json!(error.failure_class())).unwrap_or(json!("")),
        "last_failure_class": last_failure.get("failure_class").cloned().unwrap_or(json!("")),
        "error_category": result.error.as_ref().map(|error|json!(error.error_category())).unwrap_or(json!("")),
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
            "requests": result.detail.requests,
        },
        "discovery": {
            "list_items":result.detail.list_items, "date_filtered":result.detail.date_filtered,
            "raw_date_filtered":result.detail.date_filtered,
            "retained_items":result.detail.discovery_items.len(),
            "normal_overlap_count":result.detail.overlap_decisions.len(),
            "overlap_decisions":result.detail.overlap_decisions,
            "discovery_seconds":result.detail.discovery_seconds,
            "date_filter_seconds":result.detail.date_filter_seconds,
            "detail_fetch_seconds":result.detail.fetch_seconds,
            "items":result.detail.item_records,
            "filter_decisions":result.detail.filter_decisions,
            "content_filtered_out":result.detail.filter_decisions.len(),
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
                    let http_started = Instant::now();
                    let mut fetched = client.fetch_detail_text(&item.link, &policy).await;
                    let http_seconds = http_started.elapsed().as_secs_f64();
                    let mut fallback = None;
                    let mut browser_seconds = 0.0;
                    if policy.browser_fallback && !fetched.as_ref().is_ok_and(|body| !adapters::parse_detail_full_text(&item.source, body).is_empty()) {
                        let initial_reason = fetched.as_ref().err().map_or("empty_detail_text", |error| error.failure_class().as_str());
                        let browser_started = Instant::now();
                        let wait_script = policy.browser_wait_script();
                        let rendered = crate::browser::fetch_rendered_html_after_with_transport(&item.link, wait_script.as_deref(), Some(policy.timeout), policy.browser_native_user_agent, policy.browser_entry_url.as_deref())
                            .await.map_err(classify_browser_error);
                        browser_seconds = browser_started.elapsed().as_secs_f64();
                        fallback = Some(json!({"source":item.source,"url":item.link,"http_reason":initial_reason,
                            "wait_for_selector":policy.browser_wait_for_selector,
                            "browser_status":if rendered.is_ok(){"success"}else{"failed"},
                            "browser_failure_class":rendered.as_ref().err().map(|error|error.failure_class().as_str())}));
                        fetched = rendered;
                    }
                    let parse_started = Instant::now();
                    let issue = match fetched {
                        Ok(body) => {
                            let full_text = adapters::parse_detail_full_text(&item.source, &body);
                            if full_text.is_empty() {
                                Some(json!({"source": item.source, "url": item.link, "reason": adapters::detail_empty_reason(&item.source, &body)}))
                            } else {
                                item.summary = full_text.clone();
                                item.full_text = full_text;
                                None
                            }
                        }
                        Err(error) => Some(json!({"source": item.source, "url": item.link, "reason": error.failure_class().as_str()})),
                    };
                    let request = json!({"source":item.source,"url":item.link,"http_seconds":http_seconds,
                        "browser_seconds":browser_seconds,"parse_seconds":parse_started.elapsed().as_secs_f64(),
                        "status":if issue.is_none(){"recovered"}else{"failed"},"reason":issue.as_ref().and_then(|value|value.get("reason"))});
                    (item, true, issue, fallback, Some(request))
                } else {
                    (item, false, None, None, None)
                }
            }
        })
        .buffered(4)
        .collect()
        .await;
    let mut detail = DetailDiagnostics::default();
    let mut items = Vec::with_capacity(outcomes.len());
    for (item, attempted, issue, fallback, request) in outcomes {
        if let Some(request) = request {
            detail.requests.push(request);
        }
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
        result.detail.requests = diagnostics.requests;
    } else {
        for item in &mut result.items {
            item.full_text.clear();
        }
    }
    result.detail.item_records =
        result
            .items
            .iter()
            .zip(&result.detail.discovery_items)
            .enumerate()
            .map(|(index, (item, discovery))| {
                let status =
                    if mode == crate::ContentMode::Summary {
                        "not_needed"
                    } else if !item.full_text.is_empty() {
                        "fetched"
                    } else if result.detail.issues.iter().any(|issue| {
                        issue["url"] == item.link && issue["reason"] == "attachment_only"
                    }) {
                        "attachment_only"
                    } else if item.link.is_empty() {
                        "not_needed"
                    } else {
                        "failed"
                    };
                json!({"source": item.source, "date": item.date, "title": item.title,
            "link": item.link, "list_summary": discovery.summary, "detail_status": status,
            "route": result.detail.item_records.get(index).and_then(|record| record.get("route")),
            "publisher":result.detail.item_records.get(index).and_then(|record|record.get("publisher")),
            "categories":result.detail.item_records.get(index).and_then(|record|record.get("categories")),
            "origins":result.detail.item_records.get(index).and_then(|record|record.get("origins"))})
            })
            .collect();
}

pub(super) fn parse_date(value: &str) -> Option<NaiveDate> {
    if let Ok(timestamp) = chrono::NaiveDateTime::parse_from_str(value.trim(), "%m/%d/%Y %H:%M:%S")
    {
        // Official no-offset timestamps represent the source's Taipei publication day.
        return Some(timestamp.date());
    }
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

    fn list_page(items: &[(&str, &str, &str)]) -> String {
        items
            .iter()
            .map(|(id, title, date)| {
                format!(
            "<article><a href='/article/{id}'><h2>{title}</h2></a><time>{date}</time></article>"
        )
            })
            .collect()
    }

    async fn paginated_result(
        feed: Option<&str>,
        pages: Vec<(String, u16, String)>,
        max_pages: u32,
    ) -> (SourceResult, Vec<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut responses = pages;
        if let Some(body) = feed {
            responses.insert(0, ("/rss".into(), 200, body.into()));
        }
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + std::time::Duration::from_secs(10);
            while requests.len() < responses.len() && Instant::now() < deadline {
                let (mut socket, _) = match listener.accept() {
                    Ok(socket) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("mock server accept: {error}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 4096];
                let bytes = match socket.read(&mut request) {
                    Ok(0) => continue,
                    Ok(bytes) => bytes,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue
                    }
                    Err(error) => panic!("mock server read: {error}"),
                };
                let request = String::from_utf8_lossy(&request[..bytes]);
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                let response = responses.iter().find(|(expected, _, _)| expected == &path);
                let (status, body) = response
                    .map(|(_, status, body)| (*status, body.as_str()))
                    .unwrap_or((404, "unknown mock route"));
                write!(socket,"HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                requests.push(path);
            }
            requests
        });
        let mut routes = Vec::new();
        let xml = feed.is_some_and(|body| body.contains("<Datas>"));
        if feed.is_some() {
            routes.push(json!({"id":"rss","url":format!("{base}/rss"),"kind":if xml {"xml"} else {"rss"},"parser":if xml {"thb-xml"} else {"standard-rss"},"priority":1,"coverage_reduced":xml}));
        }
        routes.push(json!({"id":"list","url":format!("{base}/list?channel=culture"),"kind":"html","parser":"standard","priority":if feed.is_some(){2}else{1},
            "pagination":{"parameter":"page","first_page":1,"max_pages":max_pages},
            "selectors":{"item":"article","link":"a","title":"h2","date":"time"}}));
        let definition:SourceDefinition=serde_json::from_value(json!({"name":if xml {"公路局"} else {"分頁來源"},"urls":[format!("{base}/list?channel=culture")],"routes":routes,"transport":{"list":{"timeout_seconds":2,"retry_attempts":1}}})).unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let result = fetch_source_definition(
            &HttpClient::new().unwrap(),
            &definition,
            &definition.name,
            DateRange {
                start,
                end: start + chrono::Duration::days(6),
            },
            None,
            1,
            start,
        )
        .await;
        (result, server.join().unwrap())
    }

    #[tokio::test]
    #[ignore = "requires system Chrome and a local HTTP listener"]
    async fn browser_list_applies_declared_page_deadline() {
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stop = stopped.clone();
        let server = std::thread::spawn(move || {
            while !server_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                            .unwrap();
                        let _ = socket.read(&mut [0; 4096]);
                        let body = "<html><body>Official page without ready selector</body></html>";
                        let _ = write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock browser server: {error}"),
                }
            }
        });
        let definition: SourceDefinition = serde_json::from_value(json!({
            "name":"Browser deadline fixture", "urls":[url],
            "routes":[{"id":"browser", "url":url, "kind":"browser", "parser":"standard",
                "priority":1, "wait_for_selector":"#never-ready", "transport":{"timeout_seconds":1}}]
        })).unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
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
            date,
        )
        .await;
        stopped.store(true, Ordering::Relaxed);
        server.join().unwrap();
        let error = result.error.expect("missing selector must time out");
        assert_eq!(error.failure_class().as_str(), "browser_runtime");
        assert!(error.to_string().contains("頁面逾時（1 秒）"), "{error}");
        assert_eq!(result.attempts.len(), 1);
    }

    #[tokio::test]
    async fn xml_and_html_overlap_retains_full_text_and_all_origins() {
        let xml = include_str!("../../tests/fixtures/thb-official-news.xml");
        let route:SourceRoute=serde_json::from_value(json!({"id":"xml","url":"https://www.thb.gov.tw/","kind":"xml","parser":"thb-xml","priority":1})).unwrap();
        let records = adapters::parse_route_records("公路局", &route, xml).unwrap();
        let range = DateRange {
            start: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 10, 11).unwrap(),
        };
        let week = filter_to_date_range(
            records.into_iter().map(|record| record.item).collect(),
            range,
        );
        let html = week
            .iter()
            .take(6)
            .map(|item| {
                format!(
                    "<article><a href='{}'><h2>{}</h2></a><time>{}</time></article>",
                    item.link.replace('&', "&amp;"),
                    item.title,
                    item.date
                )
            })
            .collect::<String>();
        let (mut result, _) = paginated_result(
            Some(xml),
            vec![
                ("/list?channel=culture".into(), 200, html),
                (
                    "/list?channel=culture&page=2".into(),
                    200,
                    list_page(&[("old", "前週", "2026-10-04")]),
                ),
            ],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert_eq!(result.items.len(), week.len());
        assert_eq!(result.detail.overlap_decisions.len(), 6);
        assert_eq!(result.detail.date_filtered, result.items.len() + 6);
        assert_eq!(
            crate::scraper::quality::process(result.items.clone()).duplicate_count,
            0
        );
        for expected in &week {
            let kept = result
                .items
                .iter()
                .find(|item| item.link == expected.link)
                .unwrap();
            assert_eq!(kept.full_text, expected.full_text);
        }
        enrich_source(
            &HttpClient::new().unwrap(),
            &mut result,
            crate::ContentMode::Full,
        )
        .await;
        assert_eq!(result.detail.attempted, 0);
        assert_eq!(result.detail.item_records.len(), result.items.len());
        assert_eq!(result.detail.discovery_items.len(), result.items.len());
        let origins = result.detail.item_records[0]["origins"].as_array().unwrap();
        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0]["route"]["route_id"], "rss");
        assert_eq!(origins[1]["route"]["route_id"], "list");
        assert_eq!(origins[1]["route"]["page"], 1);
        assert!(origins[0].get("publisher").is_some());
        assert!(origins[0].get("categories").is_some());
    }

    #[tokio::test]
    async fn same_page_duplicates_remain_visible_to_quality() {
        let rows = vec![("duplicate", "同頁異常新聞", "2026-10-08"); 6];
        let (result, _) = paginated_result(
            None,
            vec![
                ("/list?channel=culture".into(), 200, list_page(&rows)),
                (
                    "/list?channel=culture&page=2".into(),
                    200,
                    list_page(&[("old", "前週", "2026-10-04")]),
                ),
            ],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert_eq!(result.items.len(), 6);
        assert!(result.detail.overlap_decisions.is_empty());
        let quality = crate::scraper::quality::process(result.items);
        assert_eq!(quality.duplicate_count, 5);
        assert!(quality.duplicate_count as f64 / quality.input_count as f64 >= 0.20);
    }

    #[test]
    fn overlap_preserves_within_page_multiplicity_and_exact_article_identity() {
        let item = NewsItem {
            source: "來源".into(),
            date: "2026-10-08".into(),
            date_source: "pubDate".into(),
            department: "來源".into(),
            title: "同一新聞".into(),
            link: "https://official.test/1".into(),
            category: "新聞".into(),
            summary: "摘要".into(),
            full_text: "完整官方全文".into(),
        };
        let mut items = Vec::new();
        let mut detail = DetailDiagnostics::default();
        let first = json!({"route_id":"rss","url":"https://official.test/rss","page":null});
        let second = json!({"route_id":"html","url":"https://official.test/list","page":1});
        let record = crate::scraper::rss::FeedRecord {
            item: item.clone(),
            publisher: Some("官方單位".into()),
            categories: vec!["新聞".into(), "活動".into()],
        };
        merge_discovered_record(&mut items, &mut detail, record.clone(), first);
        for _ in 0..6 {
            merge_discovered_record(&mut items, &mut detail, record.clone(), second.clone());
        }
        assert_eq!(items.len(), 6);
        assert_eq!(detail.overlap_decisions.len(), 1);
        assert_eq!(
            crate::scraper::quality::process(items.clone()).duplicate_count,
            5
        );
        assert_eq!(
            detail.item_records[0]["origins"][0]["publisher"],
            "官方單位"
        );
        assert_eq!(
            detail.item_records[0]["origins"][1]["categories"],
            json!(["新聞", "活動"])
        );
        let mut different = record;
        different.item.date = "2026-10-09".into();
        merge_discovered_record(
            &mut items,
            &mut detail,
            different,
            json!({"route_id":"third","page":1}),
        );
        assert_eq!(
            items.len(),
            7,
            "An identical URL with a different publication date is not proven overlap"
        );
    }

    #[tokio::test]
    async fn pinned_week_article_merges_across_pages_without_losing_new_rows() {
        let (result, _) = paginated_result(
            None,
            vec![
                (
                    "/list?channel=culture".into(),
                    200,
                    list_page(&[
                        ("pin", "置頂新聞", "2026-10-08"),
                        ("a", "新聞甲", "2026-10-07"),
                    ]),
                ),
                (
                    "/list?channel=culture&page=2".into(),
                    200,
                    list_page(&[
                        ("pin", "置頂新聞", "2026-10-08"),
                        ("b", "新聞乙", "2026-10-06"),
                    ]),
                ),
                (
                    "/list?channel=culture&page=3".into(),
                    200,
                    list_page(&[
                        ("pin", "置頂新聞", "2026-10-08"),
                        ("old", "前週", "2026-10-04"),
                    ]),
                ),
            ],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.detail.date_filtered, 5);
        assert_eq!(result.detail.overlap_decisions.len(), 2);
        assert_eq!(
            result.detail.overlap_decisions[0]["reason"],
            "cross_page_overlap"
        );
        assert_eq!(
            result.detail.item_records[0]["origins"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            crate::scraper::quality::process(result.items).duplicate_count,
            0
        );
    }

    #[tokio::test]
    async fn xml_partial_news_survives_blocked_list_without_claiming_complete_week() {
        let (result, _) = paginated_result(
            Some(include_str!("../../tests/fixtures/thb-official-news.xml")),
            vec![("/list?channel=culture".into(), 200,
                "<html><iframe src=\"/_Incapsula_Resource\">Request unsuccessful. Incapsula</iframe></html>".into())], 20,
        ).await;
        assert!(!result.items.is_empty());
        assert!(result.items.iter().all(|item| !item.full_text.is_empty()));
        assert!(matches!(result.error, Some(ScraperError::AccessBlocked(_))));
        assert_eq!(result.final_route.unwrap()["coverage_reduced"], true);
        assert!(result
            .attempts
            .iter()
            .any(|attempt| attempt["continuing_for_coverage"] == true));
    }

    #[tokio::test]
    async fn repeated_new_pin_does_not_force_paging_past_old_body_rows() {
        let (result, requests) = paginated_result(
            None,
            vec![
                (
                    "/list?channel=culture".into(),
                    200,
                    list_page(&[
                        ("pin", "新置頂公告", "2026-10-12"),
                        ("week", "本週公告", "2026-10-08"),
                    ]),
                ),
                (
                    "/list?channel=culture&page=2".into(),
                    200,
                    list_page(&[
                        ("pin", "新置頂公告", "2026-10-12"),
                        ("old", "前週公告", "2026-10-04"),
                    ]),
                ),
            ],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert_eq!(requests.len(), 2);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "本週公告");
    }

    #[tokio::test]
    async fn short_feed_supplements_week_from_paginated_list_despite_old_pins() {
        let feed="<rss><channel><item><title>RSS新聞</title><link>https://official.test/rss-news</link><pubDate>2026-10-10</pubDate></item></channel></rss>";
        let (result, requests) = paginated_result(
            Some(feed),
            vec![
                (
                    "/list?channel=culture".into(),
                    200,
                    list_page(&[
                        ("pin", "置頂舊公告", "2026-01-01"),
                        ("future", "下週公告", "2026-10-12"),
                        ("tail-pin", "頁尾舊公告", "2026-09-01"),
                    ]),
                ),
                (
                    "/list?channel=culture&page=2".into(),
                    200,
                    list_page(&[
                        ("week-end", "週末展覽", "2026-10-11"),
                        ("week-start", "週初活動", "2026-10-05"),
                    ]),
                ),
                (
                    "/list?channel=culture&page=3".into(),
                    200,
                    list_page(&[("prior", "前週公告", "2026-10-04")]),
                ),
            ],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert_eq!(
            result
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["RSS新聞", "週末展覽", "週初活動"]
        );
        assert_eq!(
            requests,
            [
                "/rss",
                "/list?channel=culture",
                "/list?channel=culture&page=2",
                "/list?channel=culture&page=3"
            ]
        );
        assert!(result
            .attempts
            .iter()
            .any(|attempt| attempt["route_id"] == "rss"
                && attempt["continuing_for_coverage"] == true));
        assert_eq!(result.final_route.unwrap()["coverage_reduced"], false);
    }

    #[tokio::test]
    async fn later_page_failure_keeps_both_feed_and_first_page_news() {
        let feed="<rss><channel><item><title>RSS新聞</title><link>https://official.test/rss-news</link><pubDate>2026-10-10</pubDate></item></channel></rss>";
        let (result, requests) = paginated_result(
            Some(feed),
            vec![
                (
                    "/list?channel=culture".into(),
                    200,
                    list_page(&[("first", "已取得的展覽", "2026-10-08")]),
                ),
                (
                    "/list?channel=culture&page=2".into(),
                    503,
                    "暫時不可用".into(),
                ),
            ],
            20,
        )
        .await;
        assert_eq!(requests.len(), 3);
        assert_eq!(
            result
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["RSS新聞", "已取得的展覽"]
        );
        assert!(matches!(result.error, Some(ScraperError::SourceOutage(_))));
        assert_eq!(
            result.final_route.as_ref().unwrap()["coverage_reduced"],
            true
        );
        let failed = result
            .attempts
            .iter()
            .find(|attempt| attempt["status"] == "failed")
            .unwrap();
        assert_eq!(failed["page"], 2);
        assert_eq!(failed["failure_class"], "source_outage");
        assert_eq!(source_diagnostic(&result)["failure_class"], "source_outage");
    }

    #[tokio::test]
    async fn repeated_article_page_has_failure_evidence_without_duplicating_partial_items() {
        let first = list_page(&[("first", "週中展覽", "2026-10-08")]);
        let (result, _) = paginated_result(
            None,
            vec![
                ("/list?channel=culture".into(), 200, first.clone()),
                ("/list?channel=culture&page=2".into(), 200, first),
            ],
            20,
        )
        .await;
        assert_eq!(result.items.len(), 1);
        assert!(matches!(
            result.error,
            Some(ScraperError::ParserRegression(_))
        ));
        let failed = result
            .attempts
            .iter()
            .find(|attempt| attempt["status"] == "failed")
            .unwrap();
        assert_eq!(failed["page"], 2);
        assert_eq!(
            failed["failure_evidence"]["stage"],
            "pagination_repeated_page"
        );
        assert_eq!(
            source_diagnostic(&result)["failure_class"],
            "parser_regression"
        );
    }

    #[tokio::test]
    async fn page_limit_is_reported_as_failure_while_retaining_fetched_news() {
        let (result, requests) = paginated_result(
            None,
            vec![(
                "/list?channel=culture".into(),
                200,
                list_page(&[("first", "週中展覽", "2026-10-08")]),
            )],
            1,
        )
        .await;
        assert_eq!(requests.len(), 1);
        assert_eq!(result.items.len(), 1);
        assert!(matches!(result.error, Some(ScraperError::SourceOutage(_))));
        let failed = result
            .attempts
            .iter()
            .find(|attempt| attempt["status"] == "failed")
            .unwrap();
        assert_eq!(failed["failure_evidence"]["stage"], "pagination_limit");
        assert_eq!(failed["failure_evidence"]["max_pages"], 1);
        assert_eq!(source_diagnostic(&result)["failure_class"], "source_outage");
        assert!(result
            .attempts
            .iter()
            .any(|attempt| attempt["status"] == "success"
                && attempt["requested_window_covered"] == false));
    }

    #[tokio::test]
    async fn valid_published_list_before_the_requested_week_is_zero_news_success() {
        let (result, requests) = paginated_result(
            None,
            vec![(
                "/list?channel=culture".into(),
                200,
                list_page(&[("prior", "前週公告", "2026-10-04")]),
            )],
            20,
        )
        .await;
        assert!(result.error.is_none());
        assert!(result.items.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(source_diagnostic(&result)["status"], "success");
    }

    #[test]
    fn custom_browser_readiness_script_is_reusable_on_every_page() {
        let route:SourceRoute=serde_json::from_value(json!({"id":"browser","url":"https://official.test/list","kind":"browser","parser":"standard","wait_for_selector":"article[data-ready=\"yes\"]"})).unwrap();
        let script = route_page_script(&route).unwrap();
        assert!(script.contains("document.querySelector(\"article[data-ready=\\\"yes\\\"]\")"));
        assert!(script.contains("official news selector not ready"));
        assert!(route_page_script(&SourceRoute {
            wait_for_selector: None,
            ..route
        })
        .is_none());
    }

    #[test]
    fn official_month_day_timestamp_retains_the_taipei_publication_date() {
        assert_eq!(
            parse_date("08/25/2026 11:01:24"),
            NaiveDate::from_ymd_opt(2026, 8, 25)
        );
        assert_eq!(
            parse_date("10/05/2026 00:01:24"),
            NaiveDate::from_ymd_opt(2026, 10, 5)
        );
        assert_eq!(
            parse_date("02/29/2024 23:59:59"),
            NaiveDate::from_ymd_opt(2024, 2, 29)
        );
        assert!(parse_date("02/30/2026 11:01:24").is_none());
    }

    #[tokio::test]
    async fn short_rss_retains_items_when_fallback_succeeds_or_fails() {
        use std::io::{Read, Write};
        for fail in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let recent = "<rss><channel><item><title>RSS最新新聞</title><link>https://example.test/recent</link><pubDate>2026-10-10</pubDate></item></channel></rss>";
            let older = "<rss><channel><item><title>同週較早新聞</title><link>https://example.test/older</link><pubDate>2026-10-05</pubDate></item><item><title>前週新聞</title><link>https://example.test/prior-week</link><pubDate>2026-10-04</pubDate></item></channel></rss>";
            let server = std::thread::spawn(move || {
                for (status, body) in [
                    ("200 OK", recent),
                    (
                        if fail {
                            "503 Service Unavailable"
                        } else {
                            "200 OK"
                        },
                        older,
                    ),
                ] {
                    let (mut socket, _) = listener.accept().unwrap();
                    let mut request = [0; 1024];
                    let _ = socket.read(&mut request);
                    write!(socket, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            let definition: SourceDefinition = serde_json::from_value(json!({"name":"測試", "urls":[format!("{base}/rss")],"routes":[
                {"id":"rss", "url":format!("{base}/rss"), "kind":"rss", "priority":1, "transport":{"retry_attempts":1}},
                {"id":"fallback", "url":format!("{base}/fallback"), "kind":"rss", "priority":2, "transport":{"retry_attempts":1}}
            ]})).unwrap();
            let date = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
            let result = fetch_source_definition(
                &HttpClient::new().unwrap(),
                &definition,
                "測試",
                DateRange {
                    start: date,
                    end: date + chrono::Duration::days(6),
                },
                None,
                1,
                date,
            )
            .await;
            server.join().unwrap();
            assert_eq!(result.items.len(), if fail { 1 } else { 2 });
            assert_eq!(result.items[0].title, "RSS最新新聞");
            assert_eq!(result.error.is_some(), fail);
            assert_eq!(result.attempts[0]["continuing_for_coverage"], true);
            assert_eq!(result.final_route.unwrap()["coverage_reduced"], fail);
        }
    }

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

    #[test]
    fn browser_navigation_failures_preserve_their_transport_class() {
        for (message, class) in [
            (
                "browser_navigation_tls: net::ERR_CERT_AUTHORITY_INVALID",
                "tls_certificate",
            ),
            (
                "browser_navigation_network: net::ERR_NAME_NOT_RESOLVED",
                "runner_network",
            ),
            (
                "browser_navigation_outage: net::ERR_CONNECTION_TIMED_OUT",
                "source_outage",
            ),
            ("CDP command 5 等待逾時", "browser_runtime"),
        ] {
            assert_eq!(
                classify_browser_error(message.into())
                    .failure_class()
                    .as_str(),
                class
            );
        }
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
            ..Default::default()
        };
        let definition = SourceDefinition {
            name: "固定來源".into(),
            detail_quality: None,
            urls: vec![format!("{base}/first"), format!("{base}/second")],
            parent_ministry: None,
            aggregate_routes: false,
            routes: vec![
                route("primary", "first", "html", 1),
                route("fallback", "second", "rss", 2),
            ],
            transport: None,
            ..Default::default()
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
            date,
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
