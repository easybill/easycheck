use std::path::PathBuf;

use async_trait::async_trait;
use tokio::fs;

use crate::options::Options;
use crate::status::status_checker::{StatusCheckResult, StatusChecker};

#[derive(Debug)]
pub(crate) struct ForceSuccessFileCheck {
    file_path: PathBuf,
}

#[async_trait]
impl StatusChecker for ForceSuccessFileCheck {
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>> {
        let force_success_file_path = options
            .force_success_file_path
            .to_owned()
            .unwrap_or_else(|| String::from("easycheck.success"));
        let file_path = PathBuf::from(force_success_file_path);
        Ok(Some(Self { file_path }))
    }

    fn check_name(&self) -> String {
        String::from("force success file")
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        log::debug!("checking force success file at {:?}", &self.file_path);
        match fs::metadata(&self.file_path).await {
            Ok(_) => {
                let check_result = StatusCheckResult::new_success().ignore_other_results();
                Ok(check_result)
            }
            Err(_) => {
                let check_result = StatusCheckResult::new_success();
                Ok(check_result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn file_present_returns_success_with_ignore() {
        let tmp = NamedTempFile::new().unwrap();
        let check = ForceSuccessFileCheck {
            file_path: tmp.path().to_path_buf(),
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
        assert!(result.ignore_other_results);
    }

    #[tokio::test]
    async fn file_absent_returns_success_without_ignore() {
        let check = ForceSuccessFileCheck {
            file_path: PathBuf::from("/tmp/easycheck_nonexistent_force_success_test"),
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
        assert!(!result.ignore_other_results);
    }

    #[test]
    fn check_name_returns_force_success_file() {
        let check = ForceSuccessFileCheck {
            file_path: PathBuf::from("/tmp/test"),
        };
        assert_eq!(check.check_name(), "force success file");
    }
}
