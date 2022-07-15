use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, Path},
    response::{AppendHeaders, IntoResponse, Redirect},
    Extension,
};
use http::{header::SET_COOKIE, StatusCode};

#[tracing::instrument]
pub async fn mock_login(
    Extension(state): Extension<Arc<crate::State>>,
    Path(ckey): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !state.config.mock_login {
        return (StatusCode::FORBIDDEN, "mock logins are disabled").into_response();
    }

    // Extra security
    if !addr.ip().is_loopback() {
        return (
            StatusCode::FORBIDDEN,
            "mock logins are only allowed on localhost",
        )
            .into_response();
    }

    let session_key = match state.create_session_for(&ckey).await {
        Ok(session_key) => session_key,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };

    (
        AppendHeaders([(
            SET_COOKIE,
            format!(
                // "session_key={session_key}; SameSite=Strict; Max-Age=31536000",
                "session_key={session_key}; SameSite=Strict; Path=/; Max-Age=31536000",
            ),
        )]),
        Redirect::temporary("/"),
    )
        .into_response()
}
