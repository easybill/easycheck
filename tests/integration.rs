mod common;

use common::easycheck_process::{allocate_port, EasycheckProcess, NEXT_CYCLE_WAIT};
use common::mock_http_server::MockHttpServer;
use common::mock_proxy_http_server::MockProxyProtocolHttpServer;
use common::mock_tcp_server::MockTcpServer;

// ---- Group 1: Startup ----

/// Before the first check cycle completes, easycheck returns 503 with "Initial Check".
#[tokio::test]
async fn initial_state_returns_503() {
    // Start a TCP listener that accepts connections but never responds,
    // keeping the HTTP check pending indefinitely (until its 5s timeout).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hanging_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                // Keep the connection alive but never respond
                tokio::spawn(async move {
                    let _keep_alive = stream;
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                });
            }
        }
    });

    let url = format!("http://127.0.0.1:{}/", hanging_port);
    let proc = EasycheckProcess::start(&["--http-url", &url]);
    proc.wait_for_ready().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains("Initial Check"));
}

/// With no failing checks configured, easycheck becomes healthy after the first cycle.
#[tokio::test]
async fn healthy_after_first_check_cycle() {
    let proc = EasycheckProcess::start(&[]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body, serde_json::json!([]));
}

// ---- Group 2: File Checks ----

/// When the maintenance file exists, easycheck returns 503.
#[tokio::test]
async fn mtc_file_present_returns_503() {
    let mtc_file = tempfile::NamedTempFile::new().unwrap();
    let mtc_path = mtc_file.path().to_str().unwrap().to_string();

    let proc = EasycheckProcess::start(&["--mtc-file-path", &mtc_path]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains("mtc file"));
}

/// When the maintenance file does not exist, easycheck returns 200.
#[tokio::test]
async fn mtc_file_absent_returns_200() {
    let proc = EasycheckProcess::start(&[
        "--mtc-file-path",
        "/tmp/easycheck_test_nonexistent_mtc_file",
    ]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// Force success file overrides a failing maintenance check.
#[tokio::test]
async fn force_success_overrides_mtc() {
    let mtc_file = tempfile::NamedTempFile::new().unwrap();
    let force_file = tempfile::NamedTempFile::new().unwrap();
    let mtc_path = mtc_file.path().to_str().unwrap().to_string();
    let force_path = force_file.path().to_str().unwrap().to_string();

    let proc = EasycheckProcess::start(&[
        "--mtc-file-path",
        &mtc_path,
        "--force-success-file-path",
        &force_path,
    ]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// Force success file overrides all failures including HTTP check failures.
#[tokio::test]
async fn force_success_overrides_all_failures() {
    let force_file = tempfile::NamedTempFile::new().unwrap();
    let force_path = force_file.path().to_str().unwrap().to_string();
    let mock = MockHttpServer::start(500).await;
    let url = mock.url();

    let proc =
        EasycheckProcess::start(&["--force-success-file-path", &force_path, "--http-url", &url]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

// ---- Group 3: HTTP Check ----

/// HTTP check passes when backend returns 200.
#[tokio::test]
async fn http_check_healthy_backend() {
    let mock = MockHttpServer::start(200).await;
    let url = mock.url();

    let proc = EasycheckProcess::start(&["--http-url", &url]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// HTTP check fails when backend returns 500.
#[tokio::test]
async fn http_check_unhealthy_backend() {
    let mock = MockHttpServer::start(500).await;
    let url = mock.url();

    let proc = EasycheckProcess::start(&["--http-url", &url]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains("http endpoint check"));
}

/// HTTP check accepts custom status codes (e.g. 204).
#[tokio::test]
async fn http_check_custom_status_codes() {
    let mock = MockHttpServer::start(204).await;
    let url = mock.url();

    let proc = EasycheckProcess::start(&["--http-url", &url, "--http-status-codes", "204"]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// HTTP check fails when backend is completely down (connection refused).
#[tokio::test]
async fn http_check_backend_down() {
    let dead_port = allocate_port();
    let url = format!("http://127.0.0.1:{}/", dead_port);

    let proc = EasycheckProcess::start(&["--http-url", &url]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
}

/// Easycheck follows backend state transitions (200 -> 500).
#[tokio::test]
async fn http_check_state_transition() {
    let mock = MockHttpServer::start(200).await;
    let url = mock.url();

    let proc = EasycheckProcess::start(&["--http-url", &url]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    mock.set_status(500);
    // Wait for easycheck to pick up the change in a subsequent check cycle
    tokio::time::sleep(NEXT_CYCLE_WAIT).await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
}

/// HTTP check passes through a PROXY PROTOCOL v1 server.
#[tokio::test]
async fn http_check_with_proxy_protocol_v1() {
    let mock = MockProxyProtocolHttpServer::start_v1(200).await;
    let url = mock.url();

    let proc =
        EasycheckProcess::start(&["--http-url", &url, "--http-proxy-protocol-version", "v1"]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// HTTP check passes through a PROXY PROTOCOL v2 server.
#[tokio::test]
async fn http_check_with_proxy_protocol_v2() {
    let mock = MockProxyProtocolHttpServer::start_v2(200).await;
    let url = mock.url();

    let proc =
        EasycheckProcess::start(&["--http-url", &url, "--http-proxy-protocol-version", "v2"]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

// ---- Group 4: TCP/Socket Check ----

/// Socket check passes when TCP server is reachable and responds.
#[tokio::test]
async fn socket_check_healthy() {
    let mock_tcp = MockTcpServer::start().await;
    let addr = format!("127.0.0.1:{}", mock_tcp.port);

    let proc = EasycheckProcess::start(&["--socket-addr", &addr]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// Socket check passes when server sends an initial banner and --read-initial-response is set.
#[tokio::test]
async fn socket_check_with_initial_banner() {
    let mock_tcp = MockTcpServer::start_with_banner("220 Welcome\r\n").await;
    let addr = format!("127.0.0.1:{}", mock_tcp.port);

    let proc =
        EasycheckProcess::start(&["--socket-addr", &addr, "--read-initial-response", "true"]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// Socket check times out when --read-initial-response is set but server never sends a banner.
#[tokio::test]
async fn socket_check_no_banner_with_read_initial_response() {
    let mock_tcp = MockTcpServer::start().await;
    let addr = format!("127.0.0.1:{}", mock_tcp.port);

    let proc =
        EasycheckProcess::start(&["--socket-addr", &addr, "--read-initial-response", "true"]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains("network connection check"));
}

/// Socket check fails when no TCP server is running (connection refused).
#[tokio::test]
async fn socket_check_connection_refused() {
    let dead_port = allocate_port();
    let addr = format!("127.0.0.1:{}", dead_port);

    let proc = EasycheckProcess::start(&["--socket-addr", &addr]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains("network connection check"));
}

// ---- Group 5: Combined ----

/// Multiple checks (HTTP + socket) all pass -> 200.
#[tokio::test]
async fn multiple_checks_all_pass() {
    let mock_http = MockHttpServer::start(200).await;
    let mock_tcp = MockTcpServer::start().await;
    let http_url = mock_http.url();
    let tcp_addr = format!("127.0.0.1:{}", mock_tcp.port);

    let proc = EasycheckProcess::start(&["--http-url", &http_url, "--socket-addr", &tcp_addr]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

/// When one of multiple checks fails, easycheck returns 503 with only the failing check.
#[tokio::test]
async fn multiple_checks_one_fails() {
    let mock_http = MockHttpServer::start(200).await;
    let http_url = mock_http.url();
    let dead_port = allocate_port();
    let tcp_addr = format!("127.0.0.1:{}", dead_port);

    let proc = EasycheckProcess::start(&["--http-url", &http_url, "--socket-addr", &tcp_addr]);
    proc.wait_for_check_cycle().await;

    let resp = reqwest::get(&proc.base_url()).await.unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("network connection check"),
        "expected socket failure in body: {}",
        body
    );
    assert!(
        !body.contains("http endpoint check"),
        "http check should not fail: {}",
        body
    );
}
