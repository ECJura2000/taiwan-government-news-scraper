use super::DateRange;
use chrono::{Datelike, Local, NaiveDate};
use chrono_tz::Asia::Taipei;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Compare a completed week with distinct, successful historical weeks using
/// the same policy. An empty week is only suspicious when this source normally
/// publishes several in a comparable window.
#[cfg(test)]
pub(super) fn volume_anomalies(
    report_dir: &Path,
    range: DateRange,
    ruleset_hash: &str,
    counts: &HashMap<String, usize>,
    diagnostics: &[Value],
) -> Vec<String> {
    volume_anomalies_for_mode(
        report_dir,
        range,
        ruleset_hash,
        counts,
        diagnostics,
        crate::ContentMode::Full,
    )
}

pub(super) fn volume_anomalies_for_mode(
    report_dir: &Path,
    range: DateRange,
    ruleset_hash: &str,
    counts: &HashMap<String, usize>,
    diagnostics: &[Value],
    content_mode: crate::ContentMode,
) -> Vec<String> {
    let completed_week = range.end < Local::now().with_timezone(&Taipei).date_naive();
    let mut histories: HashMap<String, Vec<usize>> = HashMap::new();
    let mut text_histories: HashMap<String, Vec<f64>> = HashMap::new();
    let mut seen_weeks = HashSet::new();
    let mut paths = match if completed_week {
        std::fs::read_dir(report_dir).ok()
    } else {
        None
    } {
        Some(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("news_scraper_run_") && name.ends_with(".json")
                    })
            })
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    paths.sort_by(|left, right| right.cmp(left));
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(report) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if report.get("run_id").is_some() && !path.with_extension("complete").is_file() {
            continue;
        }
        if report["status"] != "success"
            || report["relevance_policy"]["ruleset_hash"] != ruleset_hash
            || report["content_mode"].as_str().unwrap_or("full")
                != match content_mode {
                    crate::ContentMode::Full => "full",
                    crate::ContentMode::Summary => "summary",
                }
        {
            continue;
        }
        let Some(week_start) = report["week_start"].as_str() else {
            continue;
        };
        let Some(week_end) = report["week_end"].as_str() else {
            continue;
        };
        let (Ok(start), Ok(end)) = (
            NaiveDate::parse_from_str(week_start, "%Y-%m-%d"),
            NaiveDate::parse_from_str(week_end, "%Y-%m-%d"),
        ) else {
            continue;
        };
        if end >= range.start
            || start.weekday() != range.start.weekday()
            || (end - start).num_days() != (range.end - range.start).num_days()
        {
            continue;
        }
        let Some(source_counts) = report["quality"]["source_scraped_counts"].as_object() else {
            continue;
        };
        if !seen_weeks.insert((start, end)) {
            continue;
        }
        for (source, count) in source_counts {
            if let Some(count) = count.as_u64() {
                histories
                    .entry(source.clone())
                    .or_default()
                    .push(count as usize);
                let final_count = report["quality"]["source_counts"][source]
                    .as_u64()
                    .unwrap_or(0);
                if final_count > 0 {
                    let full_text = report["quality"]["source_full_text_counts"][source]
                        .as_u64()
                        .unwrap_or(0);
                    text_histories
                        .entry(source.clone())
                        .or_default()
                        .push(full_text as f64 / final_count as f64);
                }
            }
        }
        if seen_weeks.len() >= 6 {
            break;
        }
    }
    let healthy: HashSet<&str> = diagnostics
        .iter()
        .filter(|entry| entry["status"] == "success")
        .filter_map(|entry| entry["source"].as_str())
        .collect();
    let mut anomalies: Vec<_> = counts
        .iter()
        .filter_map(|(source, current)| {
            if !healthy.contains(source.as_str()) {
                return None;
            }
            let history = histories.get(source)?;
            volume_regression(source, *current, history)
        })
        .collect();
    for diagnostic in diagnostics {
        let Some(source) = diagnostic["source"].as_str() else {
            continue;
        };
        if diagnostic["status"] != "success" {
            continue;
        }
        let attempted = diagnostic["detail_fetch"]["attempted"]
            .as_u64()
            .unwrap_or(0);
        let recovered = diagnostic["detail_fetch"]["recovered"]
            .as_u64()
            .unwrap_or(0);
        if detail_regression(
            attempted,
            recovered,
            text_histories.get(source).map(Vec::as_slice).unwrap_or(&[]),
        ) {
            anomalies.push(format!("detail_coverage_regression:{source}"));
        }
    }
    anomalies.sort();
    anomalies
}

pub(super) fn zero_recovery_sources(diagnostics: &[Value]) -> Vec<String> {
    let mut sources = diagnostics
        .iter()
        .filter(|entry| entry["status"] == "success")
        .filter(|entry| entry["detail_fetch"]["attempted"].as_u64().unwrap_or(0) >= 5)
        .filter(|entry| entry["detail_fetch"]["recovered"] == 0)
        .filter_map(|entry| entry["source"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    sources.sort();
    sources
}

fn detail_regression(attempted: u64, recovered: u64, historical_rates: &[f64]) -> bool {
    attempted >= 5
        && recovered * 5 <= attempted
        && historical_rates.len() >= 3
        && historical_rates.iter().filter(|rate| **rate >= 0.7).count() >= 3
}

fn volume_regression(source: &str, current: usize, history: &[usize]) -> Option<String> {
    if history.len() < 3 {
        return None;
    }
    let mut sorted = history.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    if median >= 5 && current == 0 {
        Some(format!("source_zero_after_prior_activity:{source}"))
    } else if median >= 10 && current * 5 < median {
        Some(format!("source_volume_drop:{source}:{current}/{median}"))
    } else {
        None
    }
}

pub(super) fn write_json_report(summary: &crate::RunSummary, path: &Path) -> Result<(), String> {
    let mut report_value = serde_json::to_value(summary).map_err(|error| error.to_string())?;
    if let Some(report) = report_value.as_object_mut() {
        let final_path = Path::new(&summary.report_file);
        if let Some(stem) = final_path.file_stem().and_then(|value| value.to_str()) {
            report.insert(
                "run_id".into(),
                serde_json::json!(stem.trim_start_matches("news_scraper_run_")),
            );
            report.insert(
                "completion_file".into(),
                serde_json::json!(final_path.with_extension("complete")),
            );
        }
        report.remove("engine");
        report.remove("report_file");
        report.remove("error");
    }
    let bytes = serde_json::to_vec_pretty(&report_value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_volume_requires_stable_history() {
        assert!(volume_regression("甲", 0, &[8, 9]).is_none());
        assert!(volume_regression("甲", 0, &[0, 1, 1]).is_none());
        assert_eq!(
            volume_regression("甲", 0, &[6, 7, 8]).as_deref(),
            Some("source_zero_after_prior_activity:甲")
        );
        assert_eq!(
            volume_regression("甲", 1, &[10, 11, 12]).as_deref(),
            Some("source_volume_drop:甲:1/11")
        );
        assert!(volume_regression("甲", 8, &[10, 11, 12]).is_none());
    }

    #[test]
    fn full_text_alert_requires_prior_reliable_coverage() {
        assert!(detail_regression(5, 0, &[0.8, 0.9, 0.75]));
        assert!(detail_regression(5, 1, &[0.8, 0.9, 0.75]));
        assert!(!detail_regression(5, 2, &[0.8, 0.9, 0.75]));
        assert!(!detail_regression(5, 0, &[0.1, 0.9, 0.75]));
        assert!(!detail_regression(5, 0, &[0.8, 0.9]));
    }

    #[test]
    fn complete_detail_failure_is_visible_without_claiming_a_regression() {
        let directory = tempfile::tempdir().unwrap();
        let range = DateRange {
            start: NaiveDate::from_ymd_opt(2099, 9, 21).unwrap(),
            end: NaiveDate::from_ymd_opt(2099, 9, 27).unwrap(),
        };
        let diagnostic = serde_json::json!({
            "source": "甲",
            "status": "success",
            "detail_fetch": {"attempted": 5, "recovered": 0, "failed_or_empty": 5}
        });
        assert_eq!(
            zero_recovery_sources(std::slice::from_ref(&diagnostic)),
            vec!["甲"]
        );
        assert!(volume_anomalies(
            directory.path(),
            range,
            "rules",
            &HashMap::from([("甲".into(), 5)]),
            &[diagnostic]
        )
        .is_empty());
    }

    #[test]
    fn historical_reports_require_distinct_comparable_weeks_and_matching_ruleset() {
        let directory = tempfile::tempdir().unwrap();
        for (index, (start, end, hash)) in [
            ("2026-08-24", "2026-08-30", "current"),
            ("2026-08-31", "2026-09-06", "current"),
            ("2026-09-07", "2026-09-13", "current"),
            ("2026-09-14", "2026-09-20", "old"),
        ]
        .iter()
        .enumerate()
        {
            let document = serde_json::json!({
                "status": "success",
                "week_start": start,
                "week_end": end,
                "relevance_policy": {"ruleset_hash": hash},
                "quality": {"source_scraped_counts": {"甲": 8, "乙": 0}}
            });
            std::fs::write(
                directory
                    .path()
                    .join(format!("news_scraper_run_{index}.json")),
                serde_json::to_vec(&document).unwrap(),
            )
            .unwrap();
        }
        let range = DateRange {
            start: NaiveDate::from_ymd_opt(2099, 9, 21).unwrap(),
            end: NaiveDate::from_ymd_opt(2099, 9, 27).unwrap(),
        };
        // A current/incomplete week is never compared, even with a baseline.
        assert!(volume_anomalies(
            directory.path(),
            range,
            "current",
            &HashMap::from([("甲".into(), 0)]),
            &[serde_json::json!({"source":"甲","status":"success"})]
        )
        .is_empty());
        let completed = DateRange {
            start: NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 9, 20).unwrap(),
        };
        assert_eq!(
            volume_anomalies(
                directory.path(),
                completed,
                "current",
                &HashMap::from([("甲".into(), 0), ("乙".into(), 0)]),
                &[
                    serde_json::json!({"source":"甲","status":"success"}),
                    serde_json::json!({"source":"乙","status":"success"})
                ]
            ),
            vec!["source_zero_after_prior_activity:甲"]
        );
    }
}
