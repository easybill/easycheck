use crate::checks::force_success_file_check::ForceSuccessFileCheck;
use crate::checks::http_response_check::HttpResponseCheck;
use crate::checks::mtc_file_check::MtcFileCheck;
use crate::checks::network_connection_check::NetworkConnectionCheck;
use crate::options::Options;
use crate::status::status_checker::StatusChecker;
use crate::status::status_holder::{FailingCheck, StatusCheckResults, StatusHolder};
use axum::http::StatusCode;
use futures::future::join_all;
use tokio::time::Instant;

/// The managing service for status checks.
pub(crate) struct StatusManager {
    /// The status checkers that should be executed periodically
    /// to determine the status of the current instance.
    status_checker: Vec<Box<dyn StatusChecker>>,
    /// The holder for the current check status.
    status_holder: StatusHolder,
}

impl StatusManager {
    /// Registers a status checker into the given vec in case the construction
    /// was successful and the checker had all options present to be enabled.
    /// If a construction error occurred, the error is returned to the caller.
    fn register_checker_if_enabled<S>(
        status_checker: &mut Vec<Box<dyn StatusChecker>>,
        checker_construct_result: anyhow::Result<Option<S>>,
    ) -> anyhow::Result<()>
    where
        S: StatusChecker + 'static,
    {
        if let Some(checker) = checker_construct_result? {
            status_checker.push(Box::new(checker));
        }

        Ok(())
    }

    /// Constructs a new instance of the status manager. Initially
    /// all checks are considered as failed, and need to be executed
    /// before the status can change to success.
    pub fn from_options(options: &Options) -> anyhow::Result<Self> {
        // registers all enabled status checks
        let mut status_checker: Vec<Box<dyn StatusChecker>> = vec![];
        Self::register_checker_if_enabled(
            &mut status_checker,
            ForceSuccessFileCheck::from_options(options),
        )?;
        Self::register_checker_if_enabled(
            &mut status_checker,
            MtcFileCheck::from_options(options),
        )?;
        Self::register_checker_if_enabled(
            &mut status_checker,
            HttpResponseCheck::from_options(options),
        )?;
        Self::register_checker_if_enabled(
            &mut status_checker,
            NetworkConnectionCheck::from_options(options),
        )?;

        Ok(Self {
            status_checker,
            status_holder: StatusHolder::new_initial_failed(),
        })
    }

    /// Returns a cloned instance of the status holder used by this manager.
    pub(crate) fn status_holder(&self) -> StatusHolder {
        self.status_holder.clone()
    }

    /// Executes all registered status checks and sets the current
    /// status based on their execution results.
    pub async fn execute_status_checks(&self) {
        // execute all status checks in parallel
        let mut failed_checks: Vec<FailingCheck> = vec![];
        let check_futures: Vec<_> = self
            .status_checker
            .iter()
            .map(|checker| checker.execute_check())
            .collect();
        let results = join_all(check_futures).await;
        for (checker, result) in self.status_checker.iter().zip(results) {
            match result {
                Ok(check_result) => {
                    match check_result.failure_reason {
                        // failure reason is present and all other checks should be skipped, only
                        // return this failure reason
                        Some(failure_reason) if check_result.ignore_other_results => {
                            let failing_check =
                                FailingCheck::new_from_check(checker, failure_reason);
                            failed_checks = vec![failing_check];
                            break;
                        }
                        // failure reason is present but other checks shouldn't be skipped,
                        // register the failure reason and continue
                        Some(failure_reason) => {
                            let failing_check =
                                FailingCheck::new_from_check(checker, failure_reason);
                            failed_checks.push(failing_check);
                        }
                        // the check was successful and all other results should be skipped,
                        // remove all failure reasons and use the successful result
                        None if check_result.ignore_other_results => {
                            failed_checks.clear();
                            break;
                        }
                        // the check was successful and other checks should be considered as well,
                        // just continue looking at the other results
                        None => {}
                    }
                }
                Err(error) => {
                    // checker failed with an error, assume it's an issue that makes the backend be down
                    let failure_reason = format!("check failed with error: {}", error);
                    let failing_check = FailingCheck::new_from_check(checker, failure_reason);
                    failed_checks.push(failing_check);
                }
            }
        }

        let check_results = if failed_checks.is_empty() {
            // there are no failed checks, assume all services are ready
            StatusCheckResults {
                timestamp: Instant::now(),
                api_response_code: StatusCode::OK,
                failing_checks: vec![],
            }
        } else {
            // failed checks are present, assume it's down
            StatusCheckResults {
                timestamp: Instant::now(),
                api_response_code: StatusCode::SERVICE_UNAVAILABLE,
                failing_checks: failed_checks,
            }
        };

        // write the check results into the current status
        self.status_holder
            .update_current_status(check_results)
            .await;
    }
}
