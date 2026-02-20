use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::{Extension, Router};

pub struct MockHttpServer {
    pub port: u16,
    status_code: Arc<AtomicU16>,
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

async fn handler(Extension(status_code): Extension<Arc<AtomicU16>>) -> StatusCode {
    let code = status_code.load(Ordering::Relaxed);
    StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

impl MockHttpServer {
    /// Starts a mock HTTP server on a random port that responds with the given status code.
    pub async fn start(status: u16) -> Self {
        let status_code = Arc::new(AtomicU16::new(status));

        let app = Router::new()
            .fallback(handler)
            .layer(Extension(status_code.clone()));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });

        Self {
            port,
            status_code,
            _shutdown_tx: tx,
        }
    }

    /// Dynamically changes the status code returned by the mock.
    pub fn set_status(&self, status: u16) {
        self.status_code.store(status, Ordering::Relaxed);
    }

    /// Returns the base URL of the mock server.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }
}
