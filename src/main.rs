mod config;
mod routes;
mod state;

pub use config::Config;
pub use state::State;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::Extension,
    response::IntoResponse,
    routing::{get, get_service},
    Router,
};
use color_eyre::eyre::Context;
use http::StatusCode;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    tracing::info!("starting mothbus");

    let config = Arc::new(Config::read_from_file().context("failed to read config")?);
    let state = Arc::new(State::new().context("failed to create state")?);

    let port = config.port;

    let app = Router::new()
        .route("/", get(routes::index))
        // TODO: Filter html
        .nest(
            "/static",
            get_service(tower_http::services::ServeDir::new("dist"))
                .handle_error(handle_static_error),
        )
        .layer(Extension(config))
        .layer(Extension(state))
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::debug!("listening on {}", address);

    axum::Server::bind(&address)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn handle_static_error(error: std::io::Error) -> impl IntoResponse {
    tracing::error!("failed to serve static files: {error:#?}");

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "failed to serve a static file, this is a bug",
    )
}
