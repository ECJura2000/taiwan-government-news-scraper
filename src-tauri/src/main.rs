#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    if let Some(status) = taiwan_government_news_lib::scraper::pdf::run_worker_if_requested() {
        return status;
    }
    taiwan_government_news_lib::run();
    std::process::ExitCode::SUCCESS
}
