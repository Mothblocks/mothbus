use axum::response::{AppendHeaders, IntoResponse, Redirect};
use http::header::SET_COOKIE;

#[tracing::instrument]
pub async fn logout() -> impl IntoResponse {
    (
        AppendHeaders([(
            SET_COOKIE,
            "session_jwt=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/;",
        )]),
        Redirect::to("/"),
    )
}
