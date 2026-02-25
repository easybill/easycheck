use async_trait::async_trait;

use crate::options::Options;

/// Defines the shared behavior how status checks are executed.
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

/// The result of a status check.
pub(crate) struct StatusCheckResult {
    /// The reason why the status check failed. If present the check is
    /// considered as failed, if absent the check was successful.
    pub failure_reason: Option<String>,
    /// Indicate if results from other status checkers should be ignored
    /// and only this result should be returned.
    pub ignore_other_results: bool,
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

    /// Enables that only this result will be used to determine the service
    /// status. If this is enabled on multiple results, the first result with
    /// this flag set will be used as the final response.
    pub fn ignore_other_results(self) -> Self {
        Self {
            failure_reason: self.failure_reason,
            ignore_other_results: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_success_has_no_failure_and_no_ignore() {
        let result = StatusCheckResult::new_success();
        assert!(result.failure_reason.is_none());
        assert!(!result.ignore_other_results);
    }

    #[test]
    fn new_failure_has_failure_reason_and_no_ignore() {
        let result = StatusCheckResult::new_failure("something broke".to_string());
        assert_eq!(result.failure_reason.as_deref(), Some("something broke"));
        assert!(!result.ignore_other_results);
    }

    #[test]
    fn ignore_other_results_sets_flag() {
        let result = StatusCheckResult::new_success().ignore_other_results();
        assert!(result.ignore_other_results);
        assert!(result.failure_reason.is_none());

        let result = StatusCheckResult::new_failure("fail".to_string()).ignore_other_results();
        assert!(result.ignore_other_results);
        assert_eq!(result.failure_reason.as_deref(), Some("fail"));
    }
}
