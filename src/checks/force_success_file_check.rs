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
