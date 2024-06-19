use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode, Url};

use crate::options::Options;
use crate::status::status_checker::{StatusCheckResult, StatusChecker};

#[derive(Debug)]
pub(crate) struct HttpResponseCheck {
    endpoint: Url,
    http_method: Method,
    request_client: Client,
    up_status_codes: Vec<StatusCode>,
}

#[async_trait]
impl StatusChecker for HttpResponseCheck {
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>> {
        match options.http_check_url.to_owned() {
            None => Ok(None),
            Some(endpoint) => {
                let http_method = options.http_check_method.to_owned().unwrap_or(Method::GET);
                let up_status_codes = options
                    .http_check_response_codes
                    .to_owned()
                    .unwrap_or(vec![StatusCode::OK]);
                let request_client = Client::builder().timeout(Duration::from_secs(5)).build()?;
                Ok(Some(Self {
                    endpoint,
                    http_method,
                    request_client,
                    up_status_codes,
                }))
            }
        }
    }

    fn check_name(&self) -> String {
        format!("http endpoint check {}", &self.endpoint)
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        let response = self
            .request_client
            .request(self.http_method.clone(), self.endpoint.clone())
            .send()
            .await?;
        let response_code = response.status();
        let check_result = if self.up_status_codes.contains(&response_code) {
            StatusCheckResult::new_success()
        } else {
            StatusCheckResult::new_failure(format!("received status {}", &response_code))
        };
        Ok(check_result)
    }
}
