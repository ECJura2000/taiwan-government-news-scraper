use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use taiwan_government_news_lib::{native, RunOptions};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        println!("{}", version_text());
        return ExitCode::SUCCESS;
    }
    let command = match args.first().map(String::as_str) {
        Some("collect") => {
            args.remove(0);
            "collect"
        }
        Some("list-sources") | Some("--list-sources") => "list-sources",
        None => "collect",
        Some(value) if value.starts_with('-') => "collect",
        Some(value) => {
            eprintln!("不支援的子命令：{value}（可用 collect、list-sources）");
            return ExitCode::from(2);
        }
    };
    if command == "list-sources" {
        for source in taiwan_government_news_lib::list_sources_for_cli() {
            println!("{source}");
        }
        return ExitCode::SUCCESS;
    }

    let options = match parse_options(&args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("建立 Rust runtime 失敗");
    let cancelled = Arc::new(AtomicBool::new(false));
    let run_cancelled = cancelled.clone();
    let result = runtime.block_on(async {
        let run = native::run(&options, run_cancelled);
        tokio::pin!(run);
        tokio::select! {
            result = &mut run => result,
            signal = tokio::signal::ctrl_c() => {
                match signal {
                    Ok(()) => {
                        eprintln!("收到中止訊號，正在安全停止…");
                        cancelled.store(true, Ordering::SeqCst);
                        run.await
                    }
                    Err(error) => Err(format!("無法監聽中止訊號：{error}")),
                }
            }
        }
    });
    match result {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string(&summary).expect("摘要序列化失敗")
            );
            if options.fail_on_source_error && !summary.failed_sources.is_empty() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("Rust native engine 執行失敗：{error}");
            ExitCode::from(1)
        }
    }
}

fn version_text() -> String {
    format!("news-scraper {}", env!("CARGO_PKG_VERSION"))
}

fn parse_options(args: &[String]) -> Result<RunOptions, String> {
    let mut options = RunOptions {
        sources: Vec::new(),
        output_dir: None,
        report_dir: None,
        date: None,
        start_date: None,
        end_date: None,
        max_workers: 8,
        dedupe_affiliated: false,
        fail_on_source_error: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--headless" | "--json-summary" | "--gui" => {}
            "--dedupe-affiliated" => options.dedupe_affiliated = true,
            "--fail-on-source-error" => options.fail_on_source_error = true,
            "--max-workers" => {
                index += 1;
                let workers: u32 = args
                    .get(index)
                    .ok_or("--max-workers 需要數值")?
                    .parse()
                    .map_err(|_| "--max-workers 必須是正整數")?;
                if workers == 0 {
                    return Err("--max-workers 必須是正整數".into());
                }
                options.max_workers = workers;
            }
            "--output-dir" => {
                index += 1;
                options.output_dir = Some(args.get(index).ok_or("--output-dir 需要路徑")?.clone());
            }
            "--report-dir" => {
                index += 1;
                options.report_dir = Some(args.get(index).ok_or("--report-dir 需要路徑")?.clone());
            }
            "--date" => {
                index += 1;
                options.date = Some(args.get(index).ok_or("--date 需要日期")?.clone());
            }
            "--start-date" => {
                index += 1;
                options.start_date = Some(args.get(index).ok_or("--start-date 需要日期")?.clone());
            }
            "--end-date" => {
                index += 1;
                options.end_date = Some(args.get(index).ok_or("--end-date 需要日期")?.clone());
            }
            "--sources" => {
                index += 1;
                while index < args.len() && !args[index].starts_with("--") {
                    options.sources.push(args[index].clone());
                    index += 1;
                }
                index = index.saturating_sub(1);
            }
            unknown => return Err(format!("不支援的 Rust 參數：{unknown}")),
        }
        index += 1;
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collect_options_and_multiple_sources() {
        let options = parse_options(&[
            "--date".into(),
            "2026-08-06".into(),
            "--sources".into(),
            "財政部".into(),
            "法務部".into(),
            "--max-workers".into(),
            "3".into(),
        ])
        .unwrap();
        assert_eq!(options.date.as_deref(), Some("2026-08-06"));
        assert_eq!(options.sources, vec!["財政部", "法務部"]);
        assert_eq!(options.max_workers, 3);
    }

    #[test]
    fn rejects_zero_workers_and_unknown_arguments() {
        assert!(parse_options(&["--max-workers".into(), "0".into()]).is_err());
        assert!(parse_options(&["--unknown".into()]).is_err());
    }

    #[test]
    fn reports_workspace_package_version() {
        assert_eq!(version_text(), "news-scraper 2.1.5");
    }
}
