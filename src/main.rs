mod auth;
mod block_templates;
mod config;
mod handlebars;
mod hide_debug;
mod reserved_cache;
mod routes;
mod servers;
mod session;
mod state;

pub use config::Config;
use http::StatusCode;
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

    // Generating session token can panic, so just get it early
    tracing::info!(
        "session token length: {}",
        session::get_secret_token().len()
    );

    let address = state.config.address;
    let port = state.config.port;

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/login", get(routes::login::index))
        .route("/logout", get(routes::logout))
        .route("/mock-login/:ckey", get(routes::login::mock_login))
        .route("/oauth", get(routes::login::oauth))
        .route("/@:ckey", get(routes::user::for_ckey))
        .route("/recent-test-merges.json", get(routes::recent_test_merges))
        .route("/rank-logs", get(routes::rank_logs))
        .route("/polls", get(routes::polls::index))
        .route("/polls/:poll", get(routes::polls::for_poll))
        .route("/tickets", get(routes::tickets::index))
        .route("/tickets/@:ckey", get(routes::tickets::for_ckey))
        .route("/tickets/server/:server", get(routes::tickets::for_server))
        .route("/tickets/:round/:ticket", get(routes::tickets::for_ticket))
        .route("/tickets/:round", get(routes::tickets::for_round))
        .nest("/evasion", ban_evasion_service())
        .nest(
            "/static",
            get_service(tower_http::services::ServeDir::new("dist"))
                .route_layer(tower_layer::layer_fn(
                    block_templates::BlockTemplatesService::new,
                ))
                .handle_error(handle_static_error),
        )
        .fallback(routes::not_found.into_service())
        .layer(Extension(state))
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from((address, port));
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

#[cfg(feature = "secret-ban-evasion")]
fn ban_evasion_service() -> axum::routing::Router {
    routes::evasion::service()
}

#[cfg(not(feature = "secret-ban-evasion"))]
fn ban_evasion_service() -> axum::routing::Router {
    Router::new().route(
        "/",
        get(|| async {
            (
                StatusCode::FORBIDDEN,
                "secret-ban-evasion feature is not enabled",
            )
                .into_response()
        }),
    )
}
