use std::future::IntoFuture;
use std::process::exit;
use std::time::Duration;

use axum::routing::get;
use axum::{Extension, Router};
use clap::Parser;
use tokio::net::TcpListener;
use tokio::time;

use crate::http_api_routes::get_status;
use crate::options::Options;
use crate::status::status_manager::StatusManager;

pub(crate) mod checks;
mod http_api_routes;
pub(crate) mod options;
pub(crate) mod status;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = Options::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let status_manager = match StatusManager::from_options(&options) {
        Ok(manager) => manager,
        Err(error) => {
            log::error!(
                "Unable to construct status manager based on provided options: {}",
                error
            );
            exit(1)
        }
    };
    let axum_status_holder = status_manager.status_holder();

    let revalidation_interval = Duration::from_secs(options.revalidate_interval_seconds);
    let status_updating_task = tokio::spawn(async move {
        loop {
            status_manager.execute_status_checks().await;
            time::sleep(revalidation_interval).await;
        }
    });

    let app = Router::new()
        .route("/", get(get_status).options(get_status))
        .layer(Extension(axum_status_holder));
    let listener = TcpListener::bind(&options.bind_host).await?;
    let axum_serve_future = axum::serve(listener, app).into_future();
    eprintln!(
        "easycheck v{} listening on {}",
        env!("CARGO_PKG_VERSION"),
        &options.bind_host
    );

    let exit_code = tokio::select! {
        _ = status_updating_task => {
            log::error!("Status updater task failed");
            100
        }
        _ = axum_serve_future => {
            log::error!("Serving http endpoint failed");
            101
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Quit signal received, exiting!");
            0
        }
    };

    exit(exit_code)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::{Extension, Router};
    use tower::ServiceExt;

    use crate::http_api_routes::get_status;
    use crate::options::Options;
    use crate::status::status_holder::FailingCheck;
    use crate::status::status_manager::StatusManager;

    fn test_options(mtc_path: Option<String>, force_success_path: Option<String>) -> Options {
        Options {
            bind_host: "127.0.0.1:0".to_string(),
            revalidate_interval_seconds: 5,
            force_success_file_path: force_success_path,
            mtc_check_file_path: mtc_path,
            socket_check_addr: None,
            socket_check_read_initial_response: None,
            http_check_url: None,
            http_check_method: None,
            http_check_response_codes: None,
            http_proxy_protocol_version: None,
        }
    }

    fn build_app(status_manager: &StatusManager) -> Router {
        Router::new()
            .route("/", get(get_status).options(get_status))
            .layer(Extension(status_manager.status_holder()))
    }

    async fn do_request(app: Router) -> (StatusCode, Vec<FailingCheck>) {
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let checks: Vec<FailingCheck> = serde_json::from_slice(&body).unwrap();
        (status, checks)
    }

    #[tokio::test]
    async fn healthy_returns_200_empty_array() {
        let options = test_options(
            Some("/tmp/easycheck_test_nonexistent_mtc".to_string()),
            Some("/tmp/easycheck_test_nonexistent_force".to_string()),
        );
        let manager = StatusManager::from_options(&options).unwrap();
        manager.execute_status_checks().await;

        let app = build_app(&manager);
        let (status, checks) = do_request(app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(checks.is_empty());
    }

    #[tokio::test]
    async fn mtc_file_present_returns_503() {
        let mtc_file = tempfile::NamedTempFile::new().unwrap();
        let options = test_options(
            Some(mtc_file.path().to_str().unwrap().to_string()),
            Some("/tmp/easycheck_test_nonexistent_force".to_string()),
        );
        let manager = StatusManager::from_options(&options).unwrap();
        manager.execute_status_checks().await;

        let app = build_app(&manager);
        let (status, checks) = do_request(app).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!checks.is_empty());
        assert!(checks.iter().any(|c| c.check_name == "mtc file"));
    }

    #[tokio::test]
    async fn force_success_overrides_mtc_failure() {
        let mtc_file = tempfile::NamedTempFile::new().unwrap();
        let force_file = tempfile::NamedTempFile::new().unwrap();
        let options = test_options(
            Some(mtc_file.path().to_str().unwrap().to_string()),
            Some(force_file.path().to_str().unwrap().to_string()),
        );
        let manager = StatusManager::from_options(&options).unwrap();
        manager.execute_status_checks().await;

        let app = build_app(&manager);
        let (status, checks) = do_request(app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(checks.is_empty());
    }
}
