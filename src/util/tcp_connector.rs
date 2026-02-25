use std::net::SocketAddr;
use std::pin::Pin;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// Combined trait for async read+write streams, needed because Rust does not
/// allow multiple non-auto traits in a single `dyn` trait object.
pub(crate) trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

#[async_trait]
pub(crate) trait TcpConnector: Send + Sync {
    async fn connect(&self, addr: &SocketAddr) -> std::io::Result<Pin<Box<dyn AsyncStream>>>;
}

pub(crate) struct RealTcpConnector;

#[async_trait]
impl TcpConnector for RealTcpConnector {
    async fn connect(&self, addr: &SocketAddr) -> std::io::Result<Pin<Box<dyn AsyncStream>>> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Box::pin(stream))
    }
}
