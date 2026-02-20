use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use crate::options::Options;
use crate::status::status_checker::{StatusCheckResult, StatusChecker};
use crate::util::tcp_connector::{RealTcpConnector, TcpConnector};

pub(crate) struct NetworkConnectionCheck {
    target_address: SocketAddr,
    read_initial_response: bool,
    connector: Box<dyn TcpConnector>,
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
                    connector: Box::new(RealTcpConnector),
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
        match timeout(Duration::from_secs(5), async {
            let mut stream = match self.connector.connect(&self.target_address).await {
                Err(err) => {
                    let failure_reason =
                        format!("error connecting to {}: {}", self.target_address, err);
                    return Ok(StatusCheckResult::new_failure(failure_reason));
                }
                Ok(stream) => stream,
            };

            if self.read_initial_response {
                if let Some(result) = Self::read_and_discard_response(&mut stream).await {
                    return Ok(result);
                }
            }

            if let Err(err) = stream.write_all(b"QUIT\n").await {
                let failure_reason = format!(
                    "error sending QUIT message to {}: {}",
                    self.target_address, err
                );
                return Ok(StatusCheckResult::new_failure(failure_reason));
            }

            // receive & discard response from server
            if let Some(result) = Self::read_and_discard_response(&mut stream).await {
                return Ok(result);
            }

            Ok(StatusCheckResult::new_success())
        })
        .await
        {
            Err(_) => {
                let failure_reason =
                    format!("timeout checking connection to {}", self.target_address);
                Ok(StatusCheckResult::new_failure(failure_reason))
            }
            Ok(result) => result,
        }
    }
}

impl NetworkConnectionCheck {
    async fn read_and_discard_response(
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> Option<StatusCheckResult> {
        let mut buffer = [0; 1024];
        if let Err(err) = stream.read(&mut buffer).await {
            let failure_reason = format!("error receiving response: {}", err);
            return Some(StatusCheckResult::new_failure(failure_reason));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::tcp_connector::AsyncStream;
    use std::io;
    use std::net::SocketAddr;
    use std::pin::Pin;

    struct MockConnector {
        stream: std::sync::Mutex<Option<Pin<Box<dyn AsyncStream>>>>,
    }

    impl MockConnector {
        fn new(stream: impl AsyncStream + 'static) -> Self {
            Self {
                stream: std::sync::Mutex::new(Some(Box::pin(stream))),
            }
        }
    }

    struct FailingConnector {
        error_kind: io::ErrorKind,
    }

    #[async_trait]
    impl TcpConnector for MockConnector {
        async fn connect(&self, _addr: &SocketAddr) -> io::Result<Pin<Box<dyn AsyncStream>>> {
            self.stream
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| io::Error::other("stream already consumed"))
        }
    }

    #[async_trait]
    impl TcpConnector for FailingConnector {
        async fn connect(&self, _addr: &SocketAddr) -> io::Result<Pin<Box<dyn AsyncStream>>> {
            Err(io::Error::new(self.error_kind, "connection refused"))
        }
    }

    fn dummy_addr() -> SocketAddr {
        "127.0.0.1:9999".parse().unwrap()
    }

    #[tokio::test]
    async fn connect_to_open_port_returns_success() {
        let mock_stream = tokio_test::io::Builder::new()
            .write(b"QUIT\n")
            .read(b"goodbye")
            .build();

        let check = NetworkConnectionCheck {
            target_address: dummy_addr(),
            read_initial_response: false,
            connector: Box::new(MockConnector::new(mock_stream)),
        };
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
    }

    #[tokio::test]
    async fn connect_to_closed_port_returns_failure() {
        let check = NetworkConnectionCheck {
            target_address: dummy_addr(),
            read_initial_response: false,
            connector: Box::new(FailingConnector {
                error_kind: io::ErrorKind::ConnectionRefused,
            }),
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
        let mock_stream = tokio_test::io::Builder::new()
            .read(b"220 Welcome\r\n")
            .write(b"QUIT\n")
            .read(b"goodbye")
            .build();

        let check = NetworkConnectionCheck {
            target_address: dummy_addr(),
            read_initial_response: true,
            connector: Box::new(MockConnector::new(mock_stream)),
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
            connector: Box::new(RealTcpConnector),
        };
        let name = check.check_name();
        assert!(name.contains("127.0.0.1:8080"));
    }
}
