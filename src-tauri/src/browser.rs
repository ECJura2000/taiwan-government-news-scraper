use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::ffi::OsString;
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
// Two independent CDP profiles avoid making a slow fallback route block every
// other dynamic source, while still keeping Chrome process pressure bounded.
static BROWSER_SEMAPHORE: Semaphore = Semaphore::const_new(2);

// Chrome wraps JSON responses in a viewer document, just as it does XML.
// Return the original JSON text so the route's existing parser receives its
// declared format; ordinary HTML and verification pages keep their full DOM.
const CAPTURE_DOCUMENT: &str = r#"(() => {
  const type = (document.contentType || '').split(';')[0].trim().toLowerCase();
  if (type === 'application/json' || type === 'text/json' || type.endsWith('+json')) {
    return document.querySelector('pre')?.textContent || document.body?.innerText || '';
  }
  return document.getElementById('webkit-xml-viewer-source-xml')?.innerHTML || document.documentElement.outerHTML;
})()"#;

/// Fetch a rendered page through the system Chromium browser DevTools Protocol.
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
    fetch_rendered_html_after_with_certificate_policy(url, page_script, false, None, false, None)
        .await
}

pub async fn fetch_rendered_html_after_with_timeout(
    url: &str,
    page_script: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    fetch_rendered_html_after_with_certificate_policy(
        url,
        page_script,
        false,
        Some(timeout),
        false,
        None,
    )
    .await
}

/// Fetch a rendered page while accepting an invalid certificate for the single
/// caller that has an explicit source-specific TLS fallback route.
pub async fn fetch_rendered_html_after_allow_invalid_certificates(
    url: &str,
    page_script: Option<&str>,
) -> Result<String, String> {
    fetch_rendered_html_after_with_certificate_policy(url, page_script, true, None, false, None)
        .await
}

pub async fn fetch_rendered_html_after_with_transport(
    url: &str,
    page_script: Option<&str>,
    timeout: Option<Duration>,
    native_user_agent: bool,
    entry_url: Option<&str>,
) -> Result<String, String> {
    fetch_rendered_html_after_with_certificate_policy(
        url,
        page_script,
        false,
        timeout,
        native_user_agent,
        entry_url,
    )
    .await
}

async fn fetch_rendered_html_after_with_certificate_policy(
    url: &str,
    page_script: Option<&str>,
    allow_invalid_certificates: bool,
    timeout: Option<Duration>,
    native_user_agent: bool,
    entry_url: Option<&str>,
) -> Result<String, String> {
    if let Some(entry) = entry_url {
        let target = url::Url::parse(url).map_err(|e| e.to_string())?;
        let entry = url::Url::parse(entry).map_err(|e| e.to_string())?;
        if entry.origin() != target.origin()
            || !matches!(entry.scheme(), "http" | "https")
            || !entry.username().is_empty()
            || entry.password().is_some()
        {
            return Err(
                "browser entry URL must be an HTTP(S) URL on the same origin without credentials"
                    .into(),
            );
        }
    }
    // Keep browser work bounded: two independent profiles let a slow fallback
    // run alongside another dynamic source without starting an unbounded number
    // of Chrome processes on low-resource runners.
    let _browser_permit = BROWSER_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| "Chrome CDP 執行序列已關閉".to_owned())?;
    let expected_host = if allow_invalid_certificates {
        Some(
            url::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned))
                .ok_or_else(|| "TLS fallback route URL 缺少有效主機".to_owned())?,
        )
    } else {
        None
    };
    let (mut child, profile_dir, endpoint) =
        launch_browser(allow_invalid_certificates, native_user_agent).await?;
    // Scope the deadline inside the owner so timeout still kills Chrome and
    // removes its profile. Semaphore queueing and browser startup are separate.
    let fetch = fetch_from_target(
        &endpoint,
        url,
        page_script,
        expected_host.as_deref(),
        entry_url,
    );
    let result = if let Some(timeout) = timeout {
        tokio::time::timeout(timeout, fetch)
            .await
            .unwrap_or_else(|_| Err(format!("Chrome CDP 頁面逾時（{} 秒）", timeout.as_secs())))
    } else {
        fetch.await
    };
    let _ = child.kill().await;
    let _ = tokio::fs::remove_dir_all(profile_dir).await;
    result
}

async fn launch_browser(
    allow_invalid_certificates: bool,
    native_user_agent: bool,
) -> Result<(Child, PathBuf, String), String> {
    let program = find_browser().ok_or_else(|| {
        "找不到系統 Chrome/Chromium/Edge；browser route 需要可執行的 Chromium 瀏覽器".to_owned()
    })?;
    let profile_dir = std::env::temp_dir().join(format!(
        "taiwan-news-cdp-{}-{}",
        std::process::id(),
        PROFILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::create_dir_all(&profile_dir)
        .await
        .map_err(|error| format!("CDP profile 建立失敗：{error}"))?;
    let mut command = Command::new(program);
    command.args([
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
    ]);
    if !native_user_agent {
        command.arg("--user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36");
    }
    if allow_invalid_certificates {
        command.arg("--ignore-certificate-errors");
    }
    let child = command
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
    expected_host: Option<&str>,
    entry_url: Option<&str>,
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
    send_command(&mut socket, 4_001, "Network.enable", json!({})).await?;
    wait_for_response(&mut socket, 4_001).await?;
    send_command(
        &mut socket,
        4_002,
        "Page.setLifecycleEventsEnabled",
        json!({"enabled":true}),
    )
    .await?;
    wait_for_response(&mut socket, 4_002).await?;
    if let Some(entry) = entry_url {
        // Preserve the ordinary homepage-to-news navigation state in this
        // request's isolated profile. The requested page still passes all
        // normal verification and parser checks below.
        navigate_document(&mut socket, 5_000, entry).await?;
        tokio::time::sleep(Duration::from_secs(7)).await;
    }
    let mut navigation = navigate_document(&mut socket, 5, url).await?;
    // Navigation can emit its load lifecycle event before its command response.
    // Keep the loader ID so a previous entry page cannot satisfy this wait.
    wait_for_document_load(&mut socket, &mut navigation).await?;
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let mut pending_script = page_script;
    for attempt in 0..30_u64 {
        let id = 6 + attempt;
        send_command(
            &mut socket,
            id,
            "Runtime.evaluate",
            json!({"expression":CAPTURE_DOCUMENT, "returnByValue":true}),
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
        let blocked = crate::scraper::adapters::is_access_blocked_document(html);
        let challenge = crate::scraper::adapters::is_access_challenge_document(html);
        if blocked && !challenge {
            let _ = socket.close(None).await;
            return Err(
                "access_blocked: official endpoint returned an access-denied document".into(),
            );
        }
        if !blocked && !challenge {
            if let Some(expected_host) = expected_host {
                send_command(
                    &mut socket,
                    id + 1_000,
                    "Runtime.evaluate",
                    json!({"expression":"location.hostname", "returnByValue":true}),
                )
                .await?;
                let location = wait_for_response(&mut socket, id + 1_000).await?;
                let actual_host = location
                    .pointer("/result/result/value")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if actual_host != expected_host {
                    return Err(format!("TLS fallback 跳轉至未授權主機：{actual_host}"));
                }
            }
            // Wait for the official document before waiting for its news DOM.
            // A verification page cannot satisfy a source-specific selector.
            if let Some(script) = pending_script.take() {
                send_command(
                    &mut socket,
                    id + 100,
                    "Runtime.evaluate",
                    json!({"expression":script, "returnByValue":true, "awaitPromise":true}),
                )
                .await?;
                let response = wait_for_response(&mut socket, id + 100).await?;
                if let Some(exception) = response.pointer("/result/exceptionDetails") {
                    return Err(format!("official page script failed: {exception}"));
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
            let _ = socket.close(None).await;
            return Ok(html.to_owned());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let _ = socket.close(None).await;
    Err("access_blocked: 瀏覽器等待官方驗證頁超過 30 秒".to_owned())
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
        .map_err(|_| format!("CDP command {id} 等待逾時"))?
}

#[derive(Default)]
struct NavigationState {
    target_url: String,
    document_requests: Vec<String>,
    loader_id: Option<String>,
    loaded_loaders: Vec<String>,
    loaded: bool,
}

impl NavigationState {
    fn observe(&mut self, payload: &Value) -> Result<(), String> {
        match payload.get("method").and_then(Value::as_str) {
            Some("Network.requestWillBeSent")
                if payload.pointer("/params/type").and_then(Value::as_str) == Some("Document") =>
            {
                // Match the requested navigation (redirects retain requestId),
                // not unrelated iframe, image or analytics failures.
                if payload
                    .pointer("/params/request/url")
                    .and_then(Value::as_str)
                    == Some(self.target_url.as_str())
                {
                    if let Some(id) = payload.pointer("/params/requestId").and_then(Value::as_str) {
                        self.document_requests.push(id.to_owned());
                    }
                }
            }
            Some("Network.loadingFailed") => {
                let id = payload.pointer("/params/requestId").and_then(Value::as_str);
                if id.is_some_and(|id| self.document_requests.iter().any(|known| known == id)) {
                    let error = payload
                        .pointer("/params/errorText")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown document network error");
                    return Err(navigation_error(error));
                }
            }
            Some("Page.lifecycleEvent")
                if payload.pointer("/params/name").and_then(Value::as_str) == Some("load") =>
            {
                if let Some(loader) = payload.pointer("/params/loaderId").and_then(Value::as_str) {
                    self.loaded_loaders.push(loader.to_owned());
                    self.loaded |= self.loader_id.as_deref() == Some(loader);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn navigation_error(error: &str) -> String {
    let prefix = if error.starts_with("net::ERR_CERT_") {
        "browser_navigation_tls"
    } else if matches!(
        error,
        "net::ERR_NAME_NOT_RESOLVED"
            | "net::ERR_CONNECTION_REFUSED"
            | "net::ERR_ADDRESS_UNREACHABLE"
            | "net::ERR_NETWORK_CHANGED"
            | "net::ERR_INTERNET_DISCONNECTED"
            | "net::ERR_PROXY_CONNECTION_FAILED"
            | "net::ERR_TUNNEL_CONNECTION_FAILED"
    ) {
        "browser_navigation_network"
    } else if matches!(
        error,
        "net::ERR_TIMED_OUT"
            | "net::ERR_CONNECTION_TIMED_OUT"
            | "net::ERR_CONNECTION_RESET"
            | "net::ERR_CONNECTION_CLOSED"
            | "net::ERR_EMPTY_RESPONSE"
    ) {
        "browser_navigation_outage"
    } else {
        // Cancellation, protocol or renderer errors are not proof that the
        // runner's network failed. Keep them visible as browser runtime errors.
        "browser_navigation_runtime"
    };
    format!("{prefix}: {error}")
}

async fn navigate_document(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: u64,
    url: &str,
) -> Result<NavigationState, String> {
    send_command(socket, id, "Page.navigate", json!({"url": url})).await?;
    let receive = async {
        let mut state = NavigationState {
            target_url: url.to_owned(),
            ..Default::default()
        };
        while let Some(message) = socket.next().await {
            let message = message.map_err(|error| format!("CDP websocket 讀取失敗：{error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let payload: Value = serde_json::from_str(&text)
                .map_err(|error| format!("CDP 回應 JSON 無效：{error}"))?;
            state.observe(&payload)?;
            if payload.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = payload.get("error") {
                    return Err(format!("CDP command error：{error}"));
                }
                if let Some(error) = payload.pointer("/result/errorText").and_then(Value::as_str) {
                    return Err(navigation_error(error));
                }
                state.loader_id = payload
                    .pointer("/result/loaderId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                state.loaded = state
                    .loader_id
                    .as_ref()
                    .is_none_or(|loader| state.loaded_loaders.contains(loader));
                return Ok(state);
            }
        }
        Err("CDP websocket 已關閉".into())
    };
    tokio::time::timeout(Duration::from_secs(30), receive)
        .await
        .map_err(|_| {
            format!("CDP Page.navigate command {id} 等待逾時（{url}）；尚無文件網路失敗證據")
        })?
}

async fn wait_for_document_load(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    state: &mut NavigationState,
) -> Result<(), String> {
    if state.loaded {
        return Ok(());
    }
    let receive = async {
        while let Some(message) = socket.next().await {
            let message = message.map_err(|error| format!("CDP websocket 讀取失敗：{error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let payload: Value = serde_json::from_str(&text)
                .map_err(|error| format!("CDP 回應 JSON 無效：{error}"))?;
            state.observe(&payload)?;
            if state.loaded {
                return Ok(());
            }
        }
        Err("CDP websocket 已關閉".into())
    };
    // Some official pages keep a subresource pending indefinitely. Once the
    // document navigation succeeded, continue to the DOM/verification checks
    // after a bounded load wait; an observed document network failure is fatal.
    tokio::time::timeout(Duration::from_secs(30), receive)
        .await
        .unwrap_or(Ok(()))
}

fn find_browser() -> Option<PathBuf> {
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    browser_candidates(
        std::env::var_os("NEWS_SCRAPER_CHROME_BIN"),
        std::env::var_os("ProgramFiles"),
        std::env::var_os("ProgramFiles(x86)"),
        std::env::var_os("LOCALAPPDATA"),
        &path_dirs,
    )
    .into_iter()
    .find(|path| path.is_file())
}

fn browser_candidates(
    configured: Option<OsString>,
    program_files: Option<OsString>,
    program_files_x86: Option<OsString>,
    local_app_data: Option<OsString>,
    path_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(configured) = configured {
        candidates.push(PathBuf::from(configured));
    }

    candidates.extend(
        [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/microsoft-edge",
            "/opt/homebrew/bin/chromium",
        ]
        .into_iter()
        .map(PathBuf::from),
    );

    for root in [program_files, program_files_x86]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
    {
        candidates.push(root.join("Google/Chrome/Application/chrome.exe"));
        candidates.push(root.join("Chromium/Application/chrome.exe"));
        candidates.push(root.join("Microsoft/Edge/Application/msedge.exe"));
    }
    if let Some(root) = local_app_data.map(PathBuf::from) {
        candidates.push(root.join("Google/Chrome/Application/chrome.exe"));
        candidates.push(root.join("Chromium/Application/chrome.exe"));
        candidates.push(root.join("Microsoft/Edge/Application/msedge.exe"));
    }

    for directory in path_dirs {
        for executable in [
            "google-chrome",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "chrome.exe",
            "chromium.exe",
            "msedge.exe",
        ] {
            candidates.push(directory.join(executable));
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_network_evidence_ignores_unrelated_documents_and_subresources() {
        let mut state = NavigationState {
            target_url: "https://official.example/news".into(),
            ..Default::default()
        };
        for (id, kind, url) in [
            ("iframe", "Document", "https://other.example/frame"),
            ("image", "Image", "https://official.example/news"),
        ] {
            state.observe(&json!({"method":"Network.requestWillBeSent", "params":{"requestId":id,"type":kind,"request":{"url":url}}})).unwrap();
            state.observe(&json!({"method":"Network.loadingFailed", "params":{"requestId":id,"errorText":"net::ERR_CONNECTION_RESET"}})).unwrap();
        }
        state.observe(&json!({"method":"Network.requestWillBeSent", "params":{"requestId":"news","type":"Document","request":{"url":"https://official.example/news"}}})).unwrap();
        assert_eq!(state.observe(&json!({"method":"Network.loadingFailed", "params":{"requestId":"news","errorText":"net::ERR_CONNECTION_TIMED_OUT"}})).unwrap_err(), "browser_navigation_outage: net::ERR_CONNECTION_TIMED_OUT");
    }

    #[test]
    fn navigation_error_classes_require_explicit_chrome_evidence() {
        assert_eq!(
            navigation_error("net::ERR_CERT_AUTHORITY_INVALID"),
            "browser_navigation_tls: net::ERR_CERT_AUTHORITY_INVALID"
        );
        assert_eq!(
            navigation_error("net::ERR_CONNECTION_RESET"),
            "browser_navigation_outage: net::ERR_CONNECTION_RESET"
        );
        assert_eq!(
            navigation_error("net::ERR_CONNECTION_REFUSED"),
            "browser_navigation_network: net::ERR_CONNECTION_REFUSED"
        );
        assert_eq!(
            navigation_error("net::ERR_ABORTED"),
            "browser_navigation_runtime: net::ERR_ABORTED"
        );
    }

    #[test]
    fn entry_page_load_cannot_satisfy_target_navigation() {
        let mut state = NavigationState {
            loader_id: Some("news".into()),
            ..Default::default()
        };
        state
            .observe(
                &json!({"method":"Page.lifecycleEvent","params":{"name":"load","loaderId":"home"}}),
            )
            .unwrap();
        assert!(!state.loaded);
        state
            .observe(
                &json!({"method":"Page.lifecycleEvent","params":{"name":"load","loaderId":"news"}}),
            )
            .unwrap();
        assert!(state.loaded);
    }

    #[tokio::test]
    #[ignore = "requires a system Chrome or Chromium executable"]
    async fn unreachable_document_reports_chrome_connection_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = fetch_rendered_html(&format!("http://{address}/news"))
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "browser_navigation_network: net::ERR_CONNECTION_REFUSED"
        );
    }

    #[tokio::test]
    async fn navigation_response_exposes_network_error_instead_of_runtime_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.next().await.unwrap().unwrap();
            socket.send(Message::Text(json!({"id":5,"result":{"frameId":"main","errorText":"net::ERR_NAME_NOT_RESOLVED"}}).to_string().into())).await.unwrap();
        });
        let (mut socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
        let error = navigate_document(&mut socket, 5, "https://official.example/news")
            .await
            .err()
            .unwrap();
        assert_eq!(
            error,
            "browser_navigation_network: net::ERR_NAME_NOT_RESOLVED"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn navigation_preserves_load_event_received_before_command_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.next().await.unwrap().unwrap();
            for payload in [
                json!({"method":"Page.lifecycleEvent","params":{"name":"load","loaderId":"news","timestamp":1}}),
                json!({"id":5,"result":{"frameId":"main","loaderId":"news"}}),
            ] {
                socket
                    .send(Message::Text(payload.to_string().into()))
                    .await
                    .unwrap();
            }
        });
        let (mut socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
        let mut state = navigate_document(&mut socket, 5, "https://official.example/news")
            .await
            .unwrap();
        assert!(state.loaded);
        wait_for_document_load(&mut socket, &mut state)
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[test]
    fn configured_browser_is_the_first_candidate() {
        let candidates = browser_candidates(
            Some(OsString::from("/custom/chrome")),
            None,
            None,
            None,
            &[],
        );
        assert_eq!(candidates.first(), Some(&PathBuf::from("/custom/chrome")));
    }

    #[test]
    fn discovers_standard_windows_browser_locations() {
        let candidates = browser_candidates(
            None,
            Some(OsString::from("/program-files")),
            Some(OsString::from("/program-files-x86")),
            Some(OsString::from("/local-app-data")),
            &[],
        );
        assert!(candidates.contains(&PathBuf::from(
            "/program-files/Google/Chrome/Application/chrome.exe"
        )));
        assert!(candidates.contains(&PathBuf::from(
            "/program-files-x86/Microsoft/Edge/Application/msedge.exe"
        )));
        assert!(candidates.contains(&PathBuf::from(
            "/local-app-data/Chromium/Application/chrome.exe"
        )));
    }

    #[test]
    fn discovers_browser_commands_from_path() {
        let directory = PathBuf::from("/tools/bin");
        let candidates = browser_candidates(None, None, None, None, &[directory]);
        assert!(candidates.contains(&PathBuf::from("/tools/bin/google-chrome")));
        assert!(candidates.contains(&PathBuf::from("/tools/bin/msedge.exe")));
    }

    #[test]
    fn discovered_browser_paths_are_files() {
        if let Some(path) = find_browser() {
            assert!(path.is_file());
        }
    }

    #[test]
    fn nlma_browser_fallback_parses_real_json_news() {
        let source = crate::scraper::catalog::find_source("國土管理署").unwrap();
        let route = source
            .routes
            .iter()
            .find(|route| route.id == "official-json-browser-fallback")
            .expect("verified same-endpoint browser fallback");
        assert_eq!(route.kind, "browser");
        assert_eq!(route.parser, "nlma-json");
        let records = crate::scraper::adapters::parse_route_records(
            &source.name,
            route,
            include_str!("../tests/fixtures/nlma-browser-20261013.json"),
        )
        .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].item.date, "2026-10-08");
        assert!(records[0].item.title.starts_with("中央補助2.45億元"));
        assert_eq!(
            records[0].item.link,
            "https://www.nlma.gov.tw/ch/titlelist/news/17810"
        );
        assert!(records[0].item.full_text.starts_with("內政部次長董建宏"));
        assert_eq!(records[1].item.date, "2026-10-07");
    }

    #[tokio::test]
    #[ignore = "requires a system Chrome or Chromium executable"]
    async fn normal_entry_navigation_preserves_same_session_cookie() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 4096];
                let received = stream.read(&mut request).await.unwrap();
                let request = std::str::from_utf8(&request[..received]).unwrap();
                let is_news = request.starts_with("GET /news ");
                let body = if is_news && request.contains("official_entry=yes") {
                    "<html><head><title>News</title></head><body>entered-via-home</body></html>"
                } else {
                    "<html><head><title>Home</title></head><body>Home</body></html>"
                };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nSet-Cookie: official_entry=yes; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
                if is_news {
                    break;
                }
            }
        });
        let entry = format!("http://{address}/home");
        let captured = fetch_rendered_html_after_with_transport(
            &format!("http://{address}/news"),
            None,
            Some(Duration::from_secs(45)),
            false,
            Some(&entry),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert!(captured.contains("entered-via-home"));
    }

    #[tokio::test]
    #[ignore = "requires a system Chrome or Chromium executable"]
    async fn waits_for_challenge_before_running_official_page_script() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = r#"<!doctype html><html><head><title>Just a moment...</title></head><body>
        <script>window._cf_chl_opt = {}; /* /cdn-cgi/challenge-platform/ */
        setTimeout(() => { document.title = 'Official news'; document.body.innerHTML = '<main id="official-news">News</main>'; }, 4000);
        </script></body></html>"#;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let received = stream.read(&mut request).await.unwrap();
            assert!(received > 0);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        });
        let script = "(() => { const news = document.querySelector('#official-news'); if (!news) throw new Error('script ran before official document'); news.dataset.checked = 'yes'; })()";
        let captured = fetch_rendered_html_after_with_timeout(
            &format!("http://{address}/news"),
            Some(script),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert!(captured.contains("data-checked=\"yes\""));
        assert!(!captured.contains("Just a moment..."));
    }

    #[tokio::test]
    #[ignore = "requires a system Chrome or Chromium executable"]
    async fn captures_json_document_text_without_chrome_html_viewer() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let json_body = r#"[{"title":"原文 <b>& 符號","content":"<p>正文</p>"}]"#;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let received = stream.read(&mut request).await.unwrap();
            assert!(std::str::from_utf8(&request[..received])
                .unwrap()
                .starts_with("GET /news.json "));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json_body}",
                json_body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        });
        let captured = fetch_rendered_html_after_with_timeout(
            &format!("http://{address}/news.json"),
            None,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(captured, json_body);
        assert!(serde_json::from_str::<Value>(&captured).unwrap().is_array());
    }
}
