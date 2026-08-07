use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Semaphore;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct DevToolsTarget {
    #[serde(rename = "type")]
    target_type: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

static PROFILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static BROWSER_SEMAPHORE: Semaphore = Semaphore::const_new(1);

/// Fetch a rendered page through the system Chrome/Chromium DevTools Protocol.
///
/// The browser process is scoped to one request and is terminated before the
/// function returns. No Selenium, chromedriver, or Python runtime is involved.
pub async fn fetch_rendered_html(url: &str) -> Result<String, String> {
    fetch_rendered_html_after(url, None).await
}

pub async fn fetch_rendered_html_after(
    url: &str,
    page_script: Option<&str>,
) -> Result<String, String> {
    // GitHub-hosted Linux runners only provide two CPU cores. Starting several
    // Chrome instances at once can starve all of them before their CDP endpoint
    // is ready, so browser routes share one process slot per application.
    let _browser_permit = BROWSER_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| "Chrome CDP 執行序列已關閉".to_owned())?;
    let (mut child, profile_dir, endpoint) = launch_browser().await?;
    let result = fetch_from_target(&endpoint, url, page_script).await;
    let _ = child.kill().await;
    let _ = tokio::fs::remove_dir_all(profile_dir).await;
    result
}

async fn launch_browser() -> Result<(Child, PathBuf, String), String> {
    let program = find_browser().ok_or_else(|| {
        "找不到系統 Chrome/Chromium；browser route 需要可執行的 Chrome 或 Chromium".to_owned()
    })?;
    let profile_dir = std::env::temp_dir().join(format!(
        "taiwan-news-cdp-{}-{}",
        std::process::id(),
        PROFILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::create_dir_all(&profile_dir)
        .await
        .map_err(|error| format!("CDP profile 建立失敗：{error}"))?;
    let child = Command::new(program)
        .args([
            "--allow-pre-commit-input",
            "--headless=new",
            "--disable-background-networking",
            "--disable-background-timer-throttling",
            "--disable-backgrounding-occluded-windows",
            "--disable-client-side-phishing-detection",
            "--disable-default-apps",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            "--disable-blink-features=AutomationControlled",
            "--disable-features=IgnoreDuplicateNavs,Prewarm",
            "--disable-hang-monitor",
            "--disable-popup-blocking",
            "--disable-prompt-on-repost",
            "--disable-sync",
            "--enable-automation",
            "--enable-logging=stderr",
            "--log-level=0",
            "--no-first-run",
            "--no-service-autorun",
            "--password-store=basic",
            "--remote-allow-origins=*",
            "--test-type=webdriver",
            "--use-mock-keychain",
            "--window-size=1920,1080",
            "--user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
        ])
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("data:,")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Chrome 啟動失敗：{error}"))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| format!("CDP HTTP client 建立失敗：{error}"))?;
    // Allow low-resource CI runners up to 30 seconds to publish the endpoint.
    for _ in 0..120 {
        let active_port_file = profile_dir.join("DevToolsActivePort");
        if let Ok(contents) = tokio::fs::read_to_string(active_port_file).await {
            if let Some(port) = contents
                .lines()
                .next()
                .and_then(|value| value.parse::<u16>().ok())
            {
                let base = format!("http://127.0.0.1:{port}");
                if let Ok(response) = client.get(format!("{base}/json/list")).send().await {
                    if let Ok(targets) = response.json::<Vec<DevToolsTarget>>().await {
                        if let Some(endpoint) = targets
                            .into_iter()
                            .find(|target| target.target_type == "page")
                            .and_then(|target| target.web_socket_debugger_url)
                        {
                            return Ok((child, profile_dir, endpoint));
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let mut child = child;
    let _ = child.kill().await;
    let _ = tokio::fs::remove_dir_all(&profile_dir).await;
    Err("Chrome CDP endpoint 在期限內未就緒".into())
}

async fn fetch_from_target(
    endpoint: &str,
    url: &str,
    page_script: Option<&str>,
) -> Result<String, String> {
    let (mut socket, _) = connect_async(endpoint)
        .await
        .map_err(|error| format!("CDP websocket 連線失敗：{error}"))?;
    send_command(&mut socket, 1, "Page.enable", json!({})).await?;
    send_command(&mut socket, 2, "Runtime.enable", json!({})).await?;
    send_command(
        &mut socket,
        3,
        "Target.setAutoAttach",
        json!({"autoAttach":true, "flatten":true, "waitForDebuggerOnStart":false}),
    )
    .await?;
    wait_for_response(&mut socket, 3).await?;
    send_command(
        &mut socket,
        4,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source":"Object.defineProperty(navigator, 'webdriver', {get: () => undefined});"}),
    )
    .await?;
    wait_for_response(&mut socket, 4).await?;
    send_command(&mut socket, 5, "Page.navigate", json!({"url": url})).await?;
    wait_for_response(&mut socket, 5).await?;
    let _ = wait_for_event(&mut socket, "Page.loadEventFired").await;
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let first_capture_id = if let Some(script) = page_script {
        send_command(
            &mut socket,
            6,
            "Runtime.evaluate",
            json!({"expression":script, "returnByValue":true}),
        )
        .await?;
        wait_for_response(&mut socket, 6).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        7
    } else {
        6
    };
    for attempt in 0..30_u64 {
        let id = first_capture_id + attempt;
        send_command(
            &mut socket,
            id,
            "Runtime.evaluate",
            json!({"expression":"document.documentElement.outerHTML", "returnByValue":true}),
        )
        .await?;
        let response = wait_for_response(&mut socket, id).await?;
        let html = response
            .pointer("/result/result/value")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "CDP Runtime.evaluate 沒有回傳 HTML".to_owned())?;
        // A valid Incapsula-protected page can still reference
        // `_Incapsula_Resource`; only the actual challenge/error document is
        // a failure. Treating the resource script itself as blocked causes
        // false positives after the browser has passed the challenge.
        let blocked =
            html.contains("Request unsuccessful") || html.contains("incapsula incident id");
        if !blocked {
            let _ = socket.close(None).await;
            return Ok(html.to_owned());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let _ = socket.close(None).await;
    Err("瀏覽器等待 Incapsula 驗證超過 30 秒".to_owned())
}

async fn send_command(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(), String> {
    socket
        .send(Message::Text(
            json!({"id": id, "method": method, "params": params})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|error| format!("CDP command 傳送失敗：{error}"))
}

async fn wait_for_response(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: u64,
) -> Result<Value, String> {
    let receive = async {
        while let Some(message) = socket.next().await {
            let message = message.map_err(|error| format!("CDP websocket 讀取失敗：{error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let payload: Value = serde_json::from_str(&text)
                .map_err(|error| format!("CDP 回應 JSON 無效：{error}"))?;
            if payload.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = payload.get("error") {
                    return Err(format!("CDP command error：{error}"));
                }
                return Ok(payload);
            }
        }
        Err("CDP websocket 已關閉".into())
    };
    tokio::time::timeout(Duration::from_secs(30), receive)
        .await
        .map_err(|_| "CDP command 等待逾時".to_owned())?
}

async fn wait_for_event(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    method: &str,
) -> Result<(), String> {
    let receive = async {
        while let Some(message) = socket.next().await {
            let message = message.map_err(|error| format!("CDP websocket 讀取失敗：{error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let payload: Value = serde_json::from_str(&text)
                .map_err(|error| format!("CDP 回應 JSON 無效：{error}"))?;
            if payload.get("method").and_then(Value::as_str) == Some(method) {
                return Ok(());
            }
        }
        Err("CDP websocket 已關閉".into())
    };
    tokio::time::timeout(Duration::from_secs(30), receive)
        .await
        .map_err(|_| "CDP event 等待逾時".to_owned())?
}

fn find_browser() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("NEWS_SCRAPER_CHROME_BIN") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/opt/homebrew/bin/chromium",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovered_browser_paths_are_files() {
        if let Some(path) = find_browser() {
            assert!(path.is_file());
        }
    }
}
