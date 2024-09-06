use async_trait::async_trait;

use crate::options::Options;

/// The result of a status check.
pub(crate) struct StatusCheckResult {
    /// The reason why the status check failed. If present the check is
    /// considered as failed, if absent the check was successful.
    pub failure_reason: Option<String>,
    pub ignore_other_results: bool,
}

/// Defines the shared behaviour how status checks are executed.
#[async_trait]
pub trait StatusChecker: Send + Sync {
    /// Constructs a new instance of this checker based on the given options.
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>>
    where
        Self: Sized;

    /// Get a descriptive name of this check.
    fn check_name(&self) -> String;

    /// Called when the status check should be executed. When the status
    /// checking fails (returns Err) the check is considered as failed,
    /// but all other checks will still be executed. Only if a successful
    /// result is returned and bail_out is true, then the other checks
    /// will not be executed.
    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult>;
}

impl StatusCheckResult {
    /// Creates a new successful status check result.
    pub fn new_success() -> Self {
        Self {
            failure_reason: None,
            ignore_other_results: false,
        }
    }

    /// Creates a new failed status check result using the provided reason.
    pub fn new_failure(failure_reason: String) -> Self {
        Self {
            failure_reason: Some(failure_reason),
            ignore_other_results: false,
        }
    }

    pub fn ignore_other_results(self) -> Self {
        Self {
            failure_reason: self.failure_reason,
            ignore_other_results: true,
        }
    }
}
