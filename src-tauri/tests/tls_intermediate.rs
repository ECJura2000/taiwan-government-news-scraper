//! Exercise the actual Linux native-tls backend before enabling chain repair.
//! All keys/certificates are generated for localhost inside a disposable directory.
#![cfg(target_os = "linux")]
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

fn openssl(directory: &Path, args: &[&str]) {
    let output = Command::new("openssl")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn linux_intermediate_store_still_requires_root_and_matching_hostname() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path();
    openssl(
        base,
        &[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=Local test root",
            "-keyout",
            "root.key",
            "-out",
            "root.pem",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
        ],
    );
    openssl(
        base,
        &[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=Local test intermediate",
            "-keyout",
            "intermediate.key",
            "-out",
            "intermediate.csr",
        ],
    );
    std::fs::write(
        base.join("intermediate.ext"),
        "basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n",
    )
    .unwrap();
    openssl(
        base,
        &[
            "x509",
            "-req",
            "-in",
            "intermediate.csr",
            "-CA",
            "root.pem",
            "-CAkey",
            "root.key",
            "-set_serial",
            "2",
            "-days",
            "1",
            "-extfile",
            "intermediate.ext",
            "-out",
            "intermediate.pem",
        ],
    );
    openssl(
        base,
        &[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "leaf.key",
            "-out",
            "leaf.csr",
        ],
    );
    std::fs::write(base.join("leaf.ext"), "basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\n").unwrap();
    openssl(
        base,
        &[
            "x509",
            "-req",
            "-in",
            "leaf.csr",
            "-CA",
            "intermediate.pem",
            "-CAkey",
            "intermediate.key",
            "-set_serial",
            "3",
            "-days",
            "1",
            "-extfile",
            "leaf.ext",
            "-out",
            "leaf.pem",
        ],
    );

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut server = Command::new("openssl")
        .current_dir(base)
        .args([
            "s_server",
            "-accept",
            &format!("127.0.0.1:{port}"),
            "-cert",
            "leaf.pem",
            "-key",
            "leaf.key",
            "-www",
            "-quiet",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let root =
        reqwest::Certificate::from_pem(&std::fs::read(base.join("root.pem")).unwrap()).unwrap();
    let intermediate =
        reqwest::Certificate::from_pem(&std::fs::read(base.join("intermediate.pem")).unwrap())
            .unwrap();
    let make = || {
        reqwest::Client::builder()
            .no_proxy()
            .tls_certs_only([])
            .timeout(Duration::from_secs(5))
    };
    let root_only = make().add_root_certificate(root.clone()).build().unwrap();
    let intermediate_only = make()
        .add_root_certificate(intermediate.clone())
        .build()
        .unwrap();
    let complete = make()
        .add_root_certificate(root)
        .add_root_certificate(intermediate)
        .build()
        .unwrap();
    let url = format!("https://localhost:{port}/");
    let missing_intermediate = root_only.get(&url).send().await;
    let missing_root = intermediate_only.get(&url).send().await;
    let valid = complete.get(&url).send().await;
    let mismatched_hostname = complete
        .get(format!("https://127.0.0.1:{port}/"))
        .send()
        .await;
    let _ = server.kill();
    let _ = server.wait();
    assert!(
        missing_intermediate.is_err(),
        "leaf-only server must need the intermediate"
    );
    assert!(
        missing_root.is_err(),
        "an intermediate must not become a trust anchor"
    );
    assert!(valid.is_ok(), "complete chain must succeed: {valid:?}");
    assert!(
        mismatched_hostname.is_err(),
        "hostname verification must remain enforced"
    );
    // The same trust store rejects expiration; no PARTIAL_CHAIN or NO_CHECK_TIME flags.
    let future = (chrono::Utc::now() + chrono::Duration::days(2))
        .timestamp()
        .to_string();
    let expired = Command::new("openssl")
        .current_dir(base)
        .args([
            "verify",
            "-attime",
            &future,
            "-CAfile",
            "root.pem",
            "-untrusted",
            "intermediate.pem",
            "leaf.pem",
        ])
        .output()
        .unwrap();
    assert!(!expired.status.success());
    assert!(String::from_utf8_lossy(&expired.stderr).contains("expired"));
}
