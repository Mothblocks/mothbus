use std::{fmt::Debug, sync::Arc, time::Duration};

use axum::response::{Html, IntoResponse, Response};
use color_eyre::eyre::Context;
use handlebars::Handlebars;
use http::StatusCode;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, Row};

use crate::{
    handlebars::create_handlebars,
    hide_debug::HideDebug,
    routes::polls::PollCache,
    session::{self, Session},
    Config,
};

#[derive(Debug)]
pub struct State {
    pub config: HideDebug<Config>,
    pub handlebars: HideDebug<Handlebars<'static>>,
    pub mysql_pool: sqlx::MySqlPool,

    session_cache: Cache<String, Session>,
    user_cache: Cache<String, User>,

    pub poll_cache: HideDebug<PollCache>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct User {
    pub ckey: String,
    rank: Option<AdminRank>,
}

impl User {
    pub fn admin(&self) -> bool {
        self.rank.as_ref().map(AdminRank::admin).unwrap_or_default()
    }

    pub fn can_read_tickets(&self) -> bool {
        self.admin()
    }

    pub fn can_read_admin_only_polls(&self) -> bool {
        self.admin()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdminRank {
    name: String,
    flags: u32,
}

impl AdminRank {
    pub fn admin(&self) -> bool {
        self.flags & (1 << 1) != 0
    }
}

async fn create_mysql_pool(config: &Config) -> color_eyre::Result<sqlx::MySqlPool> {
    let db_pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&config.db_url)
        .await?;

    Ok(db_pool)
}

fn small_cache<
    K: Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
>() -> Cache<K, V> {
    Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(60))
        .build()
}

impl State {
    pub async fn new() -> color_eyre::Result<Self> {
        let config = Config::read_from_file().context("failed to read config")?;

        Ok(Self {
            handlebars: HideDebug(create_handlebars()?),
            mysql_pool: create_mysql_pool(&config).await?,

            session_cache: small_cache(),
            user_cache: small_cache(),

            poll_cache: HideDebug(PollCache::new()),

            config: HideDebug(config),
        })
    }

    pub fn render_template<T: Serialize>(&self, path: &'static str, data: T) -> Response {
        match self.handlebars.render(path, &data) {
            Ok(response) => Html(response).into_response(),
            Err(error) => {
                tracing::error!("failed to render template {path}: {error:#}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to render template. this is a bug.\n{error}"),
                )
                    .into_response()
            }
        }
    }

    pub async fn get_current_db_revision(&self) -> color_eyre::Result<(u32, u32)> {
        sqlx::query("SELECT major, minor FROM schema_revision ORDER BY date DESC")
            .fetch_all(&self.mysql_pool)
            .await
            .map(|rows| {
                let row = rows.first().expect("couldn't find revision");
                (row.get(0), row.get(1))
            })
            .map_err(|error| error.into())
    }

    #[tracing::instrument]
    pub async fn session(
        self: Arc<Self>,
        session_jwt: &str,
    ) -> color_eyre::Result<Option<Session>> {
        if let Some(session) = self.session_cache.get(&session_jwt.to_string()).await {
            return Ok(Some(session));
        }

        let session = match session::session_from_token(session_jwt) {
            Some(session) => session,
            None => return Ok(None),
        };

        self.session_cache
            .insert(session_jwt.to_string(), session)
            .await;

        Ok(Some(
            self.session_cache
                .get(&session_jwt.to_string())
                .await
                .unwrap(),
        ))
    }

    #[tracing::instrument]
    pub async fn user(self: Arc<Self>, ckey: &str) -> color_eyre::Result<User> {
        if let Some(user) = self.user_cache.get(&ckey.to_string()).await {
            return Ok(user);
        }

        let rank = match sqlx::query("SELECT rank FROM admin WHERE ckey = ?")
            .bind(ckey)
            .fetch_optional(&self.mysql_pool)
            .await
        {
            Ok(Some(admin_rank)) => {
                let rank_name: String = admin_rank.get("rank");
                let mut flags: u32 = 0;

                for rank in rank_name.split('+') {
                    match sqlx::query("SELECT flags, exclude_flags FROM admin_ranks WHERE rank = ?")
                        .bind(rank)
                        .fetch_optional(&self.mysql_pool)
                        .await
                    {
                        Ok(Some(rank)) => {
                            flags |= rank.get::<u32, _>("flags");
                            flags &= !rank.get::<u32, _>("exclude_flags");
                        }

                        Ok(None) => {}

                        Err(error) => return Err(error.into()),
                    }
                }

                Some(AdminRank {
                    name: rank_name,
                    flags,
                })
            }
            Ok(None) => None,
            Err(error) => return Err(error.into()),
        };

        let user = User {
            ckey: ckey.to_string(),
            rank,
        };

        self.user_cache.insert(ckey.to_string(), user).await;

        Ok(self.user_cache.get(&ckey.to_string()).await.unwrap())
    }

    /// Creates the session, and returns the session key
    #[tracing::instrument]
    pub async fn create_session_for(self: Arc<Self>, ckey: &str) -> color_eyre::Result<String> {
        session::new_session_token(ckey)
    }
}
