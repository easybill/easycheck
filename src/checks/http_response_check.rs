use crate::options::{Options, ProxyProtocolVersion};
use crate::status::status_checker::{StatusCheckResult, StatusChecker};
use crate::util::tcp_connector::{RealTcpConnector, TcpConnector};
use anyhow::Context;
use async_trait::async_trait;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::client::conn::http1::handshake;
use hyper::header::HOST;
use hyper::{Method, Request, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use proxy_header::{ProxiedAddress, ProxyHeader};
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;

pub(crate) struct HttpResponseCheck {
    remote_addr: SocketAddr,
    host_header_value: String,
    endpoint: Uri,
    request_line_target: String,
    http_method: Method,
    up_status_codes: Vec<StatusCode>,
    proxy_protocol_version: Option<ProxyProtocolVersion>,
    connector: Box<dyn TcpConnector>,
}

fn encode_proxy_header(version: &ProxyProtocolVersion) -> anyhow::Result<Vec<u8>> {
    let local_addr = SocketAddr::from_str("127.0.0.1:80")?;
    let local_address = ProxiedAddress::stream(local_addr, local_addr);
    let proxy_header = ProxyHeader::with_address(local_address);

    let mut buffer = Vec::<u8>::new();
    match version {
        ProxyProtocolVersion::V1 => proxy_header.encode_v1(&mut buffer)?,
        ProxyProtocolVersion::V2 => proxy_header.encode_v2(&mut buffer)?,
    }
    Ok(buffer)
}

#[async_trait]
impl StatusChecker for HttpResponseCheck {
    fn from_options(options: &Options) -> anyhow::Result<Option<Self>> {
        match options.http_check_url.to_owned() {
            None => Ok(None),
            Some(endpoint) => {
                let authority = endpoint.authority().context("invalid http check url")?;
                let remote_host = authority.host();
                let remote_port = authority.port().map(|port| port.as_u16()).unwrap_or(80);
                let remote_host = format!("{}:{}", remote_host, remote_port);
                let remote_addr = SocketAddr::from_str(&remote_host)
                    .context("http check url must contain a ip address")?;

                let host_header_value = authority.as_str().to_string();
                let http_method = options.http_check_method.to_owned().unwrap_or(Method::GET);
                let up_status_codes = options
                    .http_check_response_codes
                    .to_owned()
                    .unwrap_or(vec![StatusCode::OK]);
                let proxy_protocol_version = options.http_proxy_protocol_version.clone();

                // extracts the path and query part of the uri to use for the request line
                // GET <request_line_target> ...
                // this must start with a '/', therefore the extra logic in the mapping step,
                // as PathAndQuery.as_str() does not return a leading / if only the query part exists
                let request_line_target = endpoint
                    .path_and_query()
                    .map(|pq| {
                        let pg_str = pq.as_str();
                        if pg_str.starts_with('/') {
                            pg_str.to_string()
                        } else {
                            format!("/{}", pg_str)
                        }
                    })
                    .unwrap_or_else(|| "/".to_string());

                Ok(Some(Self {
                    remote_addr,
                    host_header_value,
                    endpoint,
                    request_line_target,
                    http_method,
                    up_status_codes,
                    proxy_protocol_version,
                    connector: Box::new(RealTcpConnector),
                }))
            }
        }
    }

    fn check_name(&self) -> String {
        format!("http endpoint check {}", &self.endpoint)
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        log::debug!(
            "checking http endpoint {} ({} {})",
            &self.endpoint,
            &self.http_method,
            &self.request_line_target
        );
        let response_code = timeout(Duration::from_secs(5), async {
            let mut remote_stream = self.connector.connect(&self.remote_addr).await?;
            if let Some(proxy_protocol_version) = &self.proxy_protocol_version {
                let proxy_protocol_data = encode_proxy_header(proxy_protocol_version)?;
                remote_stream.write_all(&proxy_protocol_data).await?;
            }

            let (mut sender, connection) = handshake(TokioIo::new(remote_stream)).await?;
            tokio::spawn(connection);

            let request = Request::builder()
                .uri(&self.request_line_target)
                .method(&self.http_method)
                .header(HOST, &self.host_header_value)
                .body(Empty::<Bytes>::new())?;
            let response = sender.send_request(request).await?;
            anyhow::Ok(response.status())
        })
        .await??;

        Ok(self.evaluate_response_code(response_code))
    }
}

impl HttpResponseCheck {
    fn evaluate_response_code(&self, response_code: StatusCode) -> StatusCheckResult {
        if self.up_status_codes.contains(&response_code) {
            StatusCheckResult::new_success()
        } else {
            StatusCheckResult::new_failure(format!("received status {}", &response_code))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::tcp_connector::AsyncStream;
    use std::io;
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

    fn make_check(up_status_codes: Vec<StatusCode>) -> HttpResponseCheck {
        HttpResponseCheck {
            remote_addr: dummy_addr(),
            host_header_value: dummy_addr().to_string(),
            endpoint: format!("http://{}/health", dummy_addr()).parse().unwrap(),
            request_line_target: "/health".to_string(),
            http_method: Method::GET,
            up_status_codes,
            proxy_protocol_version: None,
            connector: Box::new(RealTcpConnector),
        }
    }

    fn make_check_with_connector(
        up_status_codes: Vec<StatusCode>,
        proxy_protocol_version: Option<ProxyProtocolVersion>,
        connector: Box<dyn TcpConnector>,
    ) -> HttpResponseCheck {
        HttpResponseCheck {
            remote_addr: dummy_addr(),
            host_header_value: dummy_addr().to_string(),
            endpoint: format!("http://{}/health", dummy_addr()).parse().unwrap(),
            request_line_target: "/health".to_string(),
            http_method: Method::GET,
            up_status_codes,
            proxy_protocol_version,
            connector,
        }
    }

    /// Spawns a minimal HTTP/1 server on the given stream that returns the
    /// specified status code for any request it receives.
    async fn spawn_http_server(server_stream: tokio::io::DuplexStream, status: StatusCode) {
        use http_body_util::Full;
        use hyper::server::conn::http1::Builder;
        use hyper::service::service_fn;
        use hyper::{body::Bytes as HyperBytes, Response};

        let service = service_fn(move |_req: Request<hyper::body::Incoming>| async move {
            Ok::<_, hyper::Error>(
                Response::builder()
                    .status(status)
                    .body(Full::new(HyperBytes::from("ok")))
                    .unwrap(),
            )
        });

        let _ = Builder::new()
            .serve_connection(TokioIo::new(server_stream), service)
            .await;
    }

    /// Reads a proxy protocol header of known length from the server stream,
    /// then serves HTTP using hyper. Returns the captured prefix bytes.
    async fn spawn_proxy_protocol_http_server(
        mut server_stream: tokio::io::DuplexStream,
        status: StatusCode,
        prefix_len: usize,
    ) -> Vec<u8> {
        use tokio::io::AsyncReadExt;

        let mut prefix = vec![0u8; prefix_len];
        server_stream.read_exact(&mut prefix).await.unwrap();

        spawn_http_server(server_stream, status).await;
        prefix
    }

    #[test]
    fn matching_status_code_returns_success() {
        let check = make_check(vec![StatusCode::OK]);
        let result = check.evaluate_response_code(StatusCode::OK);
        assert!(result.failure_reason.is_none());
    }

    #[test]
    fn non_matching_status_code_returns_failure() {
        let check = make_check(vec![StatusCode::OK]);
        let result = check.evaluate_response_code(StatusCode::INTERNAL_SERVER_ERROR);
        assert!(result.failure_reason.is_some());
        assert!(result.failure_reason.unwrap().contains("500"));
    }

    #[test]
    fn multiple_accepted_codes() {
        let check = make_check(vec![StatusCode::OK, StatusCode::NO_CONTENT]);
        assert!(check
            .evaluate_response_code(StatusCode::OK)
            .failure_reason
            .is_none());
        assert!(check
            .evaluate_response_code(StatusCode::NO_CONTENT)
            .failure_reason
            .is_none());
        assert!(check
            .evaluate_response_code(StatusCode::NOT_FOUND)
            .failure_reason
            .is_some());
    }

    #[test]
    fn check_name_contains_endpoint() {
        let check = make_check(vec![StatusCode::OK]);
        let name = check.check_name();
        assert!(name.contains("127.0.0.1:9999"));
    }

    #[tokio::test]
    async fn http_request_succeeds() {
        let (client_stream, server_stream) = tokio::io::duplex(8192);

        tokio::spawn(spawn_http_server(server_stream, StatusCode::OK));

        let check = make_check_with_connector(
            vec![StatusCode::OK],
            None,
            Box::new(MockConnector::new(client_stream)),
        );
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());
    }

    #[tokio::test]
    async fn http_request_non_matching_status_returns_failure() {
        let (client_stream, server_stream) = tokio::io::duplex(8192);

        tokio::spawn(spawn_http_server(
            server_stream,
            StatusCode::INTERNAL_SERVER_ERROR,
        ));

        let check = make_check_with_connector(
            vec![StatusCode::OK],
            None,
            Box::new(MockConnector::new(client_stream)),
        );
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_some());
        assert!(result.failure_reason.unwrap().contains("500"));
    }

    #[tokio::test]
    async fn http_request_with_proxy_protocol_v1() {
        let expected_header = encode_proxy_header(&ProxyProtocolVersion::V1).unwrap();
        let prefix_len = expected_header.len();

        let (client_stream, server_stream) = tokio::io::duplex(8192);
        let server_handle = tokio::spawn(spawn_proxy_protocol_http_server(
            server_stream,
            StatusCode::OK,
            prefix_len,
        ));

        let check = make_check_with_connector(
            vec![StatusCode::OK],
            Some(ProxyProtocolVersion::V1),
            Box::new(MockConnector::new(client_stream)),
        );
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());

        let prefix = server_handle.await.unwrap();
        assert_eq!(prefix, expected_header);
    }

    #[tokio::test]
    async fn http_request_with_proxy_protocol_v2() {
        let expected_header = encode_proxy_header(&ProxyProtocolVersion::V2).unwrap();
        let prefix_len = expected_header.len();

        let (client_stream, server_stream) = tokio::io::duplex(8192);
        let server_handle = tokio::spawn(spawn_proxy_protocol_http_server(
            server_stream,
            StatusCode::OK,
            prefix_len,
        ));

        let check = make_check_with_connector(
            vec![StatusCode::OK],
            Some(ProxyProtocolVersion::V2),
            Box::new(MockConnector::new(client_stream)),
        );
        let result = check.execute_check().await.unwrap();
        assert!(result.failure_reason.is_none());

        let prefix = server_handle.await.unwrap();
        assert_eq!(prefix, expected_header);
    }

    #[tokio::test]
    async fn connection_failure_returns_error() {
        let check = make_check_with_connector(
            vec![StatusCode::OK],
            None,
            Box::new(FailingConnector {
                error_kind: io::ErrorKind::ConnectionRefused,
            }),
        );
        let result = check.execute_check().await;
        assert!(result.is_err());
    }
}
