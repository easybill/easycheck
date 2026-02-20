use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1::Builder;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

#[derive(Copy, Clone)]
enum ProxyVersion {
    V1,
    V2,
}

pub struct MockProxyProtocolHttpServer {
    pub port: u16,
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl MockProxyProtocolHttpServer {
    pub async fn start_v1(status: u16) -> Self {
        Self::start(status, ProxyVersion::V1).await
    }

    pub async fn start_v2(status: u16) -> Self {
        Self::start(status, ProxyVersion::V2).await
    }

    async fn start(status: u16, version: ProxyVersion) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    loop {
                        if let Ok((mut stream, _)) = listener.accept().await {
                            tokio::spawn(async move {
                                match version {
                                    ProxyVersion::V1 => read_proxy_v1(&mut stream).await,
                                    ProxyVersion::V2 => read_proxy_v2(&mut stream).await,
                                }
                                serve_http(stream, status).await;
                            });
                        }
                    }
                } => {}
                _ = rx => {}
            }
        });

        Self {
            port,
            _shutdown_tx: tx,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }
}

/// Reads a PROXY protocol v1 header (a single text line ending in \r\n).
async fn read_proxy_v1(stream: &mut TcpStream) {
    let mut buf = Vec::new();
    loop {
        let byte = stream.read_u8().await.unwrap();
        buf.push(byte);
        if buf.ends_with(b"\r\n") {
            break;
        }
    }
}

/// Reads a PROXY protocol v2 header (16-byte fixed header + variable payload).
async fn read_proxy_v2(stream: &mut TcpStream) {
    // 12 bytes signature + version/command + family/protocol + 2 bytes length
    let mut header = [0u8; 16];
    stream.read_exact(&mut header).await.unwrap();
    let remaining_len = u16::from_be_bytes([header[14], header[15]]) as usize;
    if remaining_len > 0 {
        let mut remaining = vec![0u8; remaining_len];
        stream.read_exact(&mut remaining).await.unwrap();
    }
}

/// Serves a single HTTP/1.1 request on the stream using hyper, returning the given status code.
async fn serve_http(stream: TcpStream, status: u16) {
    let status_code = hyper::StatusCode::from_u16(status).unwrap();
    let service = service_fn(
        move |_req: hyper::Request<hyper::body::Incoming>| async move {
            Ok::<_, hyper::Error>(
                hyper::Response::builder()
                    .status(status_code)
                    .body(Full::new(Bytes::from("ok")))
                    .unwrap(),
            )
        },
    );

    let _ = Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .await;
}
