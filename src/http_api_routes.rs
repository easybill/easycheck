use axum::http::header::AGE;
use axum::response::IntoResponse;
use axum::{Extension, Json};

use crate::status::status_holder::StatusHolder;

pub(crate) async fn get_status(
    Extension(status_holder): Extension<StatusHolder>,
) -> impl IntoResponse {
    let current_status = status_holder.current_status().await;
    let status_checks_age = current_status.timestamp.elapsed().as_secs();

    (
        current_status.api_response_code,
        [(AGE, status_checks_age.to_string())],
        Json(current_status.failing_checks),
    )
}
