use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, State};

pub mod core;
pub mod native;
pub mod scraper;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct RunOptions {
    #[serde(default)]
    pub sources: Vec<String>,
    pub output_dir: Option<String>,
    pub report_dir: Option<String>,
    #[serde(default = "default_workers")]
    pub max_workers: u32,
    #[serde(default)]
    pub dedupe_affiliated: bool,
    #[serde(default)]
    pub fail_on_source_error: bool,
}

fn default_workers() -> u32 {
    8
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RunSummary {
    pub status: String,
    #[serde(default)]
    pub news_count: u64,
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
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProgressEvent {
    pub kind: String,
    pub source: Option<String>,
    pub completed: Option<u32>,
    pub total: Option<u32>,
    pub message: Option<String>,
}

pub struct AppState {
    cancelled: Arc<AtomicBool>,
}

mod commands {
    use super::*;

    #[tauri::command]
    pub fn list_sources() -> Vec<String> {
        super::list_sources_for_cli()
    }

    #[tauri::command]
    pub async fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
        state.cancelled.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[tauri::command]
    pub async fn run_scrape(
        app: tauri::AppHandle,
        state: State<'_, AppState>,
        options: RunOptions,
    ) -> Result<RunSummary, String> {
        state.cancelled.store(false, Ordering::SeqCst);
        let _ = app.emit(
            "scraper-progress",
            ProgressEvent {
                kind: "started".into(),
                source: None,
                completed: Some(0),
                total: Some(if options.sources.is_empty() {
                    super::list_sources_for_cli().len() as u32
                } else {
                    options.sources.len() as u32
                }),
                message: Some("正在啟動 Rust native engine".into()),
            },
        );
        let summary = super::native::run(&options, state.cancelled.clone()).await?;
        let _ = app.emit(
            "scraper-progress",
            ProgressEvent {
                kind: "completed".into(),
                source: None,
                completed: None,
                total: None,
                message: Some(format!("執行完成：{}", summary.status)),
            },
        );
        Ok(summary)
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sources,
            commands::run_scrape,
            commands::cancel_run
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn list_sources_for_cli() -> Vec<String> {
    scraper::catalog::all_sources()
        .iter()
        .map(|source| source.name.clone())
        .collect()
}
