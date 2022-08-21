use axum::{
    extract::Path,
    response::{IntoResponse, Redirect},
};

// Eventually should have a player specific page, but let's do this for now
#[tracing::instrument]
pub async fn for_ckey(Path(ckey): Path<String>) -> impl IntoResponse {
    Redirect::temporary(&format!("/tickets/@{ckey}"))
}
