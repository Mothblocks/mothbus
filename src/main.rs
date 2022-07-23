mod auth;
mod config;
mod handlebars;
mod hide_debug;
mod routes;
mod state;

pub use config::Config;
pub use state::State;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::Extension,
    handler::Handler,
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

    let state = Arc::new(State::new().await.context("failed to create state")?);

    tracing::info!(
        "db version: {:?}",
        state
            .get_current_db_revision()
            .await
            .context("failed to get db version")?
    );

    let port = state.config.port;

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/login", get(routes::login::index))
        .route("/mock-login/:ckey", get(routes::login::mock_login))
        .route("/oauth", get(routes::login::oauth))
        .route("/tickets", get(routes::tickets::index))
        .route("/tickets/@:ckey", get(routes::tickets::for_ckey))
        .route("/tickets/:round/:ticket", get(routes::tickets::for_ticket))
        .nest(
            "/static",
            // TODO: Filter out html
            get_service(tower_http::services::ServeDir::new("dist"))
                .handle_error(handle_static_error),
        )
        .fallback(routes::not_found.into_service())
        .layer(Extension(state))
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::debug!("listening on {}", address);

    axum::Server::bind(&address)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
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
