use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, State};
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::Mutex;

pub mod core;
pub mod scraper;

const SOURCES: &[&str] = &[
    "行政院",
    "監察院",
    "司法院",
    "內政部",
    "國土管理署",
    "國家公園署",
    "國土測繪中心",
    "警政署",
    "消防署",
    "外交部",
    "僑委會",
    "陸委會",
    "國防部",
    "中科院",
    "國防院",
    "財政部",
    "金管會",
    "公平會",
    "中央銀行",
    "主計總處",
    "人事總處",
    "教育部",
    "國教院",
    "運動部",
    "法務部",
    "矯正署",
    "最高檢察署",
    "經濟部",
    "交通部",
    "觀光署",
    "公路局",
    "高速公路局",
    "航港局",
    "中央氣象署",
    "農業部",
    "農業金融署",
    "農糧署",
    "漁業署",
    "農村發展及水土保持署",
    "防檢署",
    "農科園區",
    "衛生福利部",
    "食藥署",
    "疾管署",
    "國健署",
    "社家署",
    "勞動部",
    "勞動力發展署",
    "職業安全衛生署",
    "勞動基金運用局",
    "文化部",
    "故宮",
    "數位發展部",
    "數位產業署",
    "資通安全署",
    "國家資通安全研究院",
    "環境部",
    "國發會",
    "國科會",
    "國家實驗研究院",
    "國家太空中心",
    "原民會",
    "客委會",
    "海委會",
    "海巡署",
    "艦隊分署",
    "偵防分署",
    "退輔會",
    "榮總",
    "通傳會",
    "工程會",
    "中選會",
];

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

#[derive(Debug, Error)]
enum AppError {
    #[error("執行 Python 相容引擎失敗：{0}")]
    Process(String),
    #[error("Python 相容引擎未輸出有效 JSON 摘要")]
    InvalidSummary,
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub struct AppState {
    cancelled: Arc<AtomicBool>,
    child: Mutex<Option<tokio::process::Child>>,
}

mod commands {
    use super::*;

    #[tauri::command]
    pub fn list_sources() -> Vec<String> {
        SOURCES.iter().map(|source| (*source).to_owned()).collect()
    }

    #[tauri::command]
    pub async fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
        state.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = state.child.lock().await.as_mut() {
            child.kill().await.map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    #[tauri::command]
    pub async fn run_scrape(
        app: tauri::AppHandle,
        state: State<'_, AppState>,
        options: RunOptions,
    ) -> Result<RunSummary, AppError> {
        state.cancelled.store(false, Ordering::SeqCst);
        let _ = app.emit(
            "scraper-progress",
            ProgressEvent {
                kind: "started".into(),
                source: None,
                completed: Some(0),
                total: Some(if options.sources.is_empty() {
                    SOURCES.len() as u32
                } else {
                    options.sources.len() as u32
                }),
                message: Some("正在啟動相容引擎".into()),
            },
        );

        let mut command =
            Command::new(std::env::var("PYTHON").unwrap_or_else(|_| "python3".into()));
        command
            .arg("-m")
            .arg("news_scraper")
            .arg("--headless")
            .arg("--json-summary");
        if !options.sources.is_empty() {
            command.arg("--sources").args(&options.sources);
        }
        command
            .arg("--max-workers")
            .arg(options.max_workers.to_string());
        if let Some(output_dir) = &options.output_dir {
            command.arg("--output-dir").arg(output_dir);
        }
        if let Some(report_dir) = &options.report_dir {
            command.arg("--report-dir").arg(report_dir);
        }
        if options.dedupe_affiliated {
            command.arg("--dedupe-affiliated");
        }
        if options.fail_on_source_error {
            command.arg("--fail-on-source-error");
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = command
            .spawn()
            .map_err(|error| AppError::Process(error.to_string()))?;
        state.child.lock().await.replace(child);
        loop {
            if state.cancelled.load(Ordering::SeqCst) {
                if let Some(child) = state.child.lock().await.as_mut() {
                    child
                        .kill()
                        .await
                        .map_err(|error| AppError::Process(error.to_string()))?;
                }
                state.child.lock().await.take();
                return Err(AppError::Process("執行已取消".into()));
            }
            let finished = {
                let mut child_guard = state.child.lock().await;
                child_guard
                    .as_mut()
                    .map(|child| child.try_wait())
                    .transpose()
                    .map_err(|error| AppError::Process(error.to_string()))?
                    .flatten()
                    .is_some()
            };
            if finished {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let child = state
            .child
            .lock()
            .await
            .take()
            .ok_or_else(|| AppError::Process("相容引擎程序狀態遺失".into()))?;
        let output = child
            .wait_with_output()
            .await
            .map_err(|error| AppError::Process(error.to_string()))?;
        if state.cancelled.load(Ordering::SeqCst) {
            return Err(AppError::Process("執行已取消".into()));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let summary_line = stdout
            .lines()
            .rev()
            .find(|line| line.trim_start().starts_with('{'))
            .ok_or(AppError::InvalidSummary)?;
        let mut summary: RunSummary =
            serde_json::from_str(summary_line).map_err(|_| AppError::InvalidSummary)?;
        if !output.status.success() && summary.error.is_none() {
            summary.error = Some(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
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
            child: Mutex::new(None),
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
    SOURCES.iter().map(|source| (*source).to_owned()).collect()
}
