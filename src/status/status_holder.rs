use std::sync::Arc;

use axum::http::StatusCode;
use serde::Serialize;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::status::status_checker::StatusChecker;

/// Holder of the current status check result.
#[derive(Clone, Debug)]
pub(crate) struct StatusHolder {
    /// The current status check result.
    current_status: Arc<RwLock<StatusCheckResults>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FailingCheck {
    /// The name of the check that failed.
    pub check_name: String,
    /// A descriptive reason why the check failed.
    pub failure_reason: String,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusCheckResults {
    /// The timestamp when the checks were last executed.
    pub timestamp: Instant,
    /// The current response code that should be sent back
    /// by the check endpoint to the requesting client.
    pub api_response_code: StatusCode,
    /// The checks that failed and lead to the changed response
    /// code. If empty, the response code should be 200.
    pub failing_checks: Vec<FailingCheck>,
}

impl FailingCheck {
    /// Constructs a new initially failed check status. This status
    /// is only used during the period of constructing the status
    /// manager and the first time the status checks are executed.
    pub(super) fn new_initial_failed() -> Self {
        Self {
            check_name: String::from("Initial Check"),
            failure_reason: String::from("Cannot determine status: checks weren't executed yet"),
        }
    }

    /// Constructs a new failing check info based on the given status
    /// checker and failure reason.
    #[allow(clippy::borrowed_box)]
    pub fn new_from_check(checker: &Box<dyn StatusChecker>, failure_reason: String) -> Self {
        Self {
            check_name: checker.check_name(),
            failure_reason,
        }
    }
}

impl StatusHolder {
    /// Creates a new status holder instance that has the initial check
    /// status set to failed.
    pub(super) fn new_initial_failed() -> Self {
        let initial_check_result = StatusCheckResults {
            timestamp: Instant::now(),
            api_response_code: StatusCode::SERVICE_UNAVAILABLE,
            failing_checks: vec![FailingCheck::new_initial_failed()],
        };
        let status = Arc::new(RwLock::new(initial_check_result));
        Self {
            current_status: status,
        }
    }

    /// Reads the current status check result.
    pub async fn current_status(&self) -> StatusCheckResults {
        self.current_status.read().await.clone()
    }

    /// Sets the current status check result.
    pub(super) async fn update_current_status(&self, check_results: StatusCheckResults) {
        let mut check_result_write_guard = self.current_status.write().await;
        *check_result_write_guard = check_results;
    }
}
