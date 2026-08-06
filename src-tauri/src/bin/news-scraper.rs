use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list-sources") {
        for source in taiwan_government_news_lib::list_sources_for_cli() {
            println!("{source}");
        }
        return ExitCode::SUCCESS;
    }

    let mut python_args = vec!["-m".to_owned(), "news_scraper".to_owned()];
    python_args.extend(args);
    if !python_args
        .iter()
        .any(|arg| arg == "--headless" || arg == "--gui")
    {
        python_args.push("--headless".to_owned());
    }
    let python = std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_owned());
    match Command::new(python).args(python_args).status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(1, 255) as u8),
        Err(error) => {
            eprintln!("無法啟動新聞抓取引擎：{error}");
            ExitCode::from(1)
        }
    }
}
