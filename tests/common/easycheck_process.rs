use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Revalidation interval passed to easycheck via --revalidation-interval.
const REVALIDATION_INTERVAL_SECS: u64 = 1;

/// The internal check timeout used by easycheck for individual checks (connect + I/O).
const CHECK_TIMEOUT_SECS: u64 = 5;

/// Timeout for poll-based helpers. Must account for the worst case:
/// check timeout + revalidation interval + startup overhead.
const POLL_TIMEOUT: Duration =
    Duration::from_secs(CHECK_TIMEOUT_SECS + REVALIDATION_INTERVAL_SECS * 3);

/// Duration to sleep when waiting for a subsequent check cycle (not the first one).
pub const NEXT_CYCLE_WAIT: Duration = Duration::from_secs(REVALIDATION_INTERVAL_SECS * 2);

pub struct EasycheckProcess {
    child: Option<Child>,
    pub port: u16,
}

impl EasycheckProcess {
    /// Starts the easycheck binary with `--bind 127.0.0.1:<port> --revalidation-interval 1`
    /// plus any extra arguments provided by the test.
    pub fn start(extra_args: &[&str]) -> Self {
        let port = allocate_port();
        let bind = format!("127.0.0.1:{}", port);

        let child = Command::new(env!("CARGO_BIN_EXE_easycheck"))
            .arg("--bind")
            .arg(&bind)
            .arg("--revalidation-interval")
            .arg(REVALIDATION_INTERVAL_SECS.to_string())
            .args(extra_args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start easycheck binary");

        Self {
            child: Some(child),
            port,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Polls GET / every 50ms until any HTTP response is received.
    pub async fn wait_for_ready(&self) {
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
        loop {
            if tokio::time::Instant::now() > deadline {
                panic!("easycheck did not become ready within {:?}", POLL_TIMEOUT);
            }
            if client.get(&self.base_url()).send().await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Polls GET / until the response body no longer contains "Initial Check",
    /// indicating at least one real check cycle has completed.
    pub async fn wait_for_check_cycle(&self) {
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
        loop {
            if tokio::time::Instant::now() > deadline {
                panic!(
                    "first check cycle did not complete within {:?}",
                    POLL_TIMEOUT
                );
            }
            if let Ok(resp) = client.get(self.base_url()).send().await {
                if let Ok(body) = resp.text().await {
                    if !body.contains("Initial Check") {
                        return;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for EasycheckProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Allocates a free OS port by binding to port 0 and returning the assigned port.
/// The listener is dropped immediately, freeing the port for use.
pub fn allocate_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
