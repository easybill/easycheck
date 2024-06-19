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
    let status_manager = match StatusManager::from_options(&options) {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!(
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
    println!("bound http listener to {}", &options.bind_host);

    let exit_code = tokio::select! {
        _ = status_updating_task => {
            eprintln!("Status updater task failed");
            100
        }
        _ = axum_serve_future => {
            eprintln!("Serving http endpoint failed");
            101
        }
        _ = tokio::signal::ctrl_c() => {
            println!("Quit signal received, exiting!");
            0
        }
    };

    exit(exit_code)
}
