use std::net::SocketAddr;

use axum::http::Method;
use clap::{Parser, ValueEnum};
use hyper::{StatusCode, Uri};

#[derive(ValueEnum, Debug, Clone, Eq, PartialEq)]
pub enum ProxyProtocolVersion {
    V1,
    V2,
}

#[derive(Parser, Debug, Clone)]
pub(crate) struct Options {
    #[arg(long = "bind", env = "EASYCHECK_BIND_HOST", required = true)]
    pub bind_host: String,
    #[arg(
        long = "revalidation-interval",
        env = "EASYCHECK_REVALIDATE_INTERVAL",
        default_value_t = 5
    )]
    pub revalidate_interval_seconds: u64,
    // file path for force success check
    #[arg(
        long = "force-success-file-path",
        env = "EASYCHECK_FORCE_SUCCESS_FILE_PATH"
    )]
    pub force_success_file_path: Option<String>,
    // file path for mtc check
    #[arg(long = "mtc-file-path", env = "EASYCHECK_MTC_FILE_PATH")]
    pub mtc_check_file_path: Option<String>,
    // check options for plain sockets
    #[arg(long = "socket-addr", env = "EASYCHECK_SOCKET_ADDR")]
    pub socket_check_addr: Option<SocketAddr>,
    #[arg(
        long = "read-initial-response",
        env = "EASYCHECK_READ_INITIAL_RESPONSE"
    )]
    pub socket_check_read_initial_response: Option<bool>,
    // check options for http checks
    #[arg(long = "http-url", env = "EASYCHECK_HTTP_URL")]
    pub http_check_url: Option<Uri>,
    #[arg(long = "http-method", env = "EASYCHECK_HTTP_METHOD")]
    pub http_check_method: Option<Method>,
    #[arg(long = "http-status-codes", env = "EASYCHECK_HTTP_STATUS_CODES")]
    pub http_check_response_codes: Option<Vec<StatusCode>>,
    #[arg(
        long = "http-proxy-protocol-version",
        env = "EASYCHECK_HTTP_PROXY_PROTOCOL_VERSION"
    )]
    pub http_proxy_protocol_version: Option<ProxyProtocolVersion>,
}
