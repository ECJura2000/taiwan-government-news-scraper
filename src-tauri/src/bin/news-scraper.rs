use std::process::{Command, ExitCode};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use taiwan_government_news_lib::{native, RunOptions};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list-sources") {
        for source in taiwan_government_news_lib::list_sources_for_cli() {
            println!("{source}");
        }
        return ExitCode::SUCCESS;
    }

    if let Some(index) = args.iter().position(|arg| arg == "--python-compat") {
        let mut python_args = args;
        python_args.remove(index);
        return run_python_compat(&python_args);
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
    match runtime.block_on(native::run(&options, Arc::new(AtomicBool::new(false)))) {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string(&summary).expect("摘要序列化失敗")
            );
            if summary.status == "failure" {
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

fn parse_options(args: &[String]) -> Result<RunOptions, String> {
    let mut options = RunOptions {
        sources: Vec::new(),
        output_dir: None,
        report_dir: None,
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
                options.max_workers = args
                    .get(index)
                    .ok_or("--max-workers 需要數值")?
                    .parse()
                    .map_err(|_| "--max-workers 必須是正整數")?;
            }
            "--output-dir" => {
                index += 1;
                options.output_dir = Some(args.get(index).ok_or("--output-dir 需要路徑")?.clone());
            }
            "--report-dir" => {
                index += 1;
                options.report_dir = Some(args.get(index).ok_or("--report-dir 需要路徑")?.clone());
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

fn run_python_compat(args: &[String]) -> ExitCode {
    let python = std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_owned());
    match Command::new(python)
        .arg("scripts/python_compat.py")
        .args(args)
        .status()
    {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(1, 255) as u8),
        Err(error) => {
            eprintln!("無法啟動 Python fallback：{error}");
            ExitCode::from(1)
        }
    }
}
