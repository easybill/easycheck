use crate::options::{Options, ProxyProtocolVersion};
use crate::status::status_checker::{StatusCheckResult, StatusChecker};
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
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug)]
pub(crate) struct HttpResponseCheck {
    remote_addr: SocketAddr,
    host_header_value: String,
    endpoint: Uri,
    request_line_target: String,
    http_method: Method,
    up_status_codes: Vec<StatusCode>,
    proxy_protocol_version: Option<ProxyProtocolVersion>,
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
                }))
            }
        }
    }

    fn check_name(&self) -> String {
        format!("http endpoint check {}", &self.endpoint)
    }

    async fn execute_check(&self) -> anyhow::Result<StatusCheckResult> {
        let response_code = timeout(Duration::from_secs(5), async {
            let mut remote_stream = TcpStream::connect(&self.remote_addr).await?;
            if let Some(proxy_protocol_version) = &self.proxy_protocol_version {
                let proxy_protocol_data = encode_proxy_header(proxy_protocol_version)?;
                remote_stream.write_all(&proxy_protocol_data).await?;
            }

            let (mut sender, connection) =
                handshake::<TokioIo<TcpStream>, Empty<Bytes>>(TokioIo::new(remote_stream)).await?;
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

        let check_result = if self.up_status_codes.contains(&response_code) {
            StatusCheckResult::new_success()
        } else {
            StatusCheckResult::new_failure(format!("received status {}", &response_code))
        };
        Ok(check_result)
    }
}
