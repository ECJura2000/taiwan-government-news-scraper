//! Bounded PDF text extraction in a disposable copy of the application binary.
//! The parser handles untrusted documents outside the collection process.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

const WORKER_FLAG: &str = "--internal-pdf-text";
pub const MAX_PDF_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PAGES: usize = 100;

pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
}

pub async fn extract(bytes: &[u8]) -> Result<String, &'static str> {
    let executable = std::env::current_exe().map_err(|_| "pdf_worker_failed")?;
    extract_with_executable(bytes, &executable, Duration::from_secs(10)).await
}

#[doc(hidden)]
pub async fn extract_with_executable(
    bytes: &[u8],
    executable: &Path,
    timeout: Duration,
) -> Result<String, &'static str> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err("pdf_size_limit");
    }
    if !is_pdf(bytes) {
        return Err("pdf_invalid");
    }
    let mut file = tempfile::Builder::new()
        .prefix("news-pdf-")
        .tempfile()
        .map_err(|_| "pdf_worker_failed")?;
    file.write_all(bytes).map_err(|_| "pdf_worker_failed")?;
    let child = tokio::process::Command::new(executable)
        .arg(WORKER_FLAG)
        .arg(file.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "pdf_worker_failed")?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| "pdf_timeout")?
        .map_err(|_| "pdf_worker_failed")?;
    match output.status.code() {
        Some(0) if output.stdout.len() <= MAX_TEXT_BYTES => {
            let text = String::from_utf8(output.stdout).map_err(|_| "pdf_worker_failed")?;
            if text.trim().is_empty() {
                Err("pdf_no_text")
            } else {
                Ok(text)
            }
        }
        Some(3) => Err("pdf_invalid"),
        Some(4) => Err("pdf_no_text"),
        Some(5) => Err("pdf_size_limit"),
        Some(6) => Err("pdf_encrypted"),
        _ => Err("pdf_worker_failed"),
    }
}

/// Checked before either entry point starts the CLI runtime or the Tauri GUI.
pub fn run_worker_if_requested() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.as_os_str() != WORKER_FLAG {
        return None;
    }
    let Some(path) = args.next() else {
        return Some(ExitCode::from(2));
    };
    if args.next().is_some() {
        return Some(ExitCode::from(2));
    }
    let result = (|| {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| 3_u8)?
            .take((MAX_PDF_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| 3_u8)?;
        let text = extract_bounded(&bytes)?;
        std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|_| 3_u8)
    })();
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    })
}

struct BoundedText {
    bytes: Vec<u8>,
    limit_hit: bool,
}
impl Write for BoundedText {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > MAX_TEXT_BYTES {
            self.limit_hit = true;
            return Err(std::io::Error::other("PDF text limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn extract_bounded(bytes: &[u8]) -> Result<String, u8> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(5);
    }
    if !is_pdf(bytes) {
        return Err(3);
    }
    let doc = pdf_extract::Document::load_mem(bytes).map_err(|_| 3_u8)?;
    if doc.is_encrypted() {
        return Err(6);
    }
    if doc.get_pages().len() > MAX_PAGES {
        return Err(5);
    }
    let mut buffer = BoundedText {
        bytes: Vec::new(),
        limit_hit: false,
    };
    let parsed = {
        let mut output = pdf_extract::PlainTextOutput::new(&mut buffer as &mut dyn Write);
        pdf_extract::output_doc(&doc, &mut output)
    };
    if buffer.limit_hit {
        return Err(5);
    }
    parsed.map_err(|_| 3_u8)?;
    let text = String::from_utf8(buffer.bytes).map_err(|_| 3_u8)?;
    let text = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !text.chars().any(char::is_alphanumeric) {
        return Err(4);
    }
    Ok(text)
}
