use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::options::Options;
use crate::status::status_checker::{StatusCheckResult, StatusChecker};

pub(crate) struct NetworkConnectionCheck {
    target_address: SocketAddr,
    read_initial_response: bool,
}

#[async_trait]
impl StatusChecker for NetworkConnectionCheck {
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>> {
        match options.socket_check_addr.to_owned() {
            None => Ok(None),
            Some(target_address) => {
                let read_initial_response =
                    options.socket_check_read_initial_response.unwrap_or(false);
                Ok(Some(Self {
                    target_address,
                    read_initial_response,
                }))
            }
        }
    }

    fn check_name(&self) -> String {
        format!("network connection check {}", self.target_address)
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        match timeout(
            Duration::from_secs(5),
            TcpStream::connect(&self.target_address),
        )
        .await
        {
            Err(_) => {
                // timeout
                let failure_reason = format!("timeout connecting to {}", self.target_address);
                Ok(StatusCheckResult::new_failure(failure_reason))
            }
            Ok(connect_result) => {
                match connect_result {
                    Err(err) => {
                        // issue connecting to provided host
                        let failure_reason =
                            format!("error connecting to {}: {}", self.target_address, err);
                        Ok(StatusCheckResult::new_failure(failure_reason))
                    }
                    Ok(mut tcp_stream) => {
                        if self.read_initial_response {
                            if let Some(result) =
                                self.read_and_discard_response(&mut tcp_stream).await
                            {
                                return Ok(result);
                            }
                        }

                        // connection successful
                        if let Err(err) = tcp_stream.write_all(b"QUIT\n").await {
                            let failure_reason = format!(
                                "error sending QUIT message to {}: {}",
                                self.target_address, err
                            );
                            return Ok(StatusCheckResult::new_failure(failure_reason));
                        }

                        // receive & discard response from server
                        if let Some(result) = self.read_and_discard_response(&mut tcp_stream).await
                        {
                            return Ok(result);
                        }

                        // successful check
                        Ok(StatusCheckResult::new_success())
                    }
                }
            }
        }
    }
}

impl NetworkConnectionCheck {
    async fn read_and_discard_response(
        &self,
        tcp_stream: &mut TcpStream,
    ) -> Option<StatusCheckResult> {
        let mut buffer = [0; 1024];
        if let Err(err) = tcp_stream.read(&mut buffer).await {
            let failure_reason = format!("error receiving response: {}", err);
            return Some(StatusCheckResult::new_failure(failure_reason));
        }
        None
    }
}
