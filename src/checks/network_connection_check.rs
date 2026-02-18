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
        log::debug!(
            "checking network connection to {} (read_initial_response={})",
            self.target_address,
            self.read_initial_response
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn connect_to_open_port_returns_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept connections and respond in the background
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        let check = NetworkConnectionCheck {
            target_address: addr,
            read_initial_response: false,
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
    }

    #[tokio::test]
    async fn connect_to_closed_port_returns_failure() {
        // Bind and immediately drop to get an unused port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let check = NetworkConnectionCheck {
            target_address: addr,
            read_initial_response: false,
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_some());
        assert!(result
            .failure_reason
            .unwrap()
            .contains("error connecting to"));
    }

    #[tokio::test]
    async fn read_initial_response_with_banner_returns_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let _ = stream.write_all(b"220 Welcome\r\n").await;
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        let check = NetworkConnectionCheck {
            target_address: addr,
            read_initial_response: true,
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
    }

    #[test]
    fn check_name_contains_address() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let check = NetworkConnectionCheck {
            target_address: addr,
            read_initial_response: false,
        };
        let name = check.check_name();
        assert!(name.contains("127.0.0.1:8080"));
    }
}
