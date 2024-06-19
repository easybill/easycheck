use std::io::ErrorKind;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::fs;

use crate::options::Options;
use crate::status::status_checker::{StatusCheckResult, StatusChecker};

#[derive(Debug)]
pub(crate) struct MtcFileCheck {
    file_path: PathBuf,
}

#[async_trait]
impl StatusChecker for MtcFileCheck {
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>> {
        let mtc_file_path = options
            .mtc_check_file_path
            .to_owned()
            .unwrap_or_else(|| String::from("easycheck.disabled"));
        let file_path = PathBuf::from(mtc_file_path);
        Ok(Some(Self { file_path }))
    }

    fn check_name(&self) -> String {
        String::from("mtc file")
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        match fs::metadata(&self.file_path).await {
            Ok(_) => {
                let reason = String::from("mtc file exists");
                let check_result = StatusCheckResult::new_failure(reason);
                Ok(check_result)
            }
            Err(error) => {
                if error.kind() == ErrorKind::NotFound {
                    // file does not exist, check is successful
                    Ok(StatusCheckResult::new_success())
                } else {
                    // unable to query mtc file metadata
                    let reason = format!("unable to query mtc existence: {}", error);
                    let check_result = StatusCheckResult::new_failure(reason);
                    Ok(check_result)
                }
            }
        }
    }
}
