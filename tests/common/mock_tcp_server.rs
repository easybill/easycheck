pub struct MockTcpServer {
    pub port: u16,
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl MockTcpServer {
    /// Starts a mock TCP server that accepts connections,
    /// reads incoming data (e.g. the QUIT message), and responds with "OK\n".
    pub async fn start() -> Self {
        Self::start_inner(None).await
    }

    /// Starts a mock TCP server that sends a banner before reading/responding.
    pub async fn start_with_banner(banner: &str) -> Self {
        Self::start_inner(Some(banner.to_string())).await
    }

    async fn start_inner(banner: Option<String>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    loop {
                        if let Ok((mut stream, _)) = listener.accept().await {
                            let banner = banner.clone();
                            tokio::spawn(async move {
                                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                if let Some(banner) = banner {
                                    let _ = stream.write_all(banner.as_bytes()).await;
                                }
                                let mut buf = [0u8; 1024];
                                let _ = stream.read(&mut buf).await;
                                let _ = stream.write_all(b"OK\n").await;
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
}
