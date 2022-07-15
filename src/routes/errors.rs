use std::sync::Arc;

use axum::{extract::OriginalUri, response::IntoResponse, Extension};
use http::StatusCode;
use serde::Serialize;

use crate::State;

#[derive(Serialize)]
pub struct ErrorTemplate {
    error_code: u16,
    error_message: String,
}

pub async fn make_forbidden(state: Arc<State>, message: &str) -> impl IntoResponse {
    let mut response = state.render_template(
        "error",
        ErrorTemplate {
            error_code: 403,
            error_message: message.to_owned(),
        },
    );

    *response.status_mut() = StatusCode::FORBIDDEN;
    response
}

pub async fn make_not_found(state: Arc<State>, message: &str) -> impl IntoResponse {
    let mut response = state.render_template(
        "error",
        ErrorTemplate {
            error_code: 404,
            error_message: message.to_owned(),
        },
    );

    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

pub async fn make_internal_server_error(
    state: Arc<State>,
    error: color_eyre::Report,
) -> impl IntoResponse {
    tracing::error!("internal server error: {error:#?}");

    let mut response = state.render_template(
        "error",
        ErrorTemplate {
            error_code: 500,
            error_message: format!("{error}"),
        },
    );

    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
}

#[tracing::instrument]
pub async fn not_found(
    Extension(state): Extension<Arc<State>>,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    make_not_found(state, &format!("{} not found", uri.path())).await
}
