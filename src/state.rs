use std::{fmt::Debug, sync::Arc, time::Duration};

use axum::response::{Html, IntoResponse, Response};
use color_eyre::eyre::Context;
use handlebars::Handlebars;
use http::StatusCode;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, sqlite::SqlitePoolOptions, Row};

use crate::{handlebars::create_handlebars, hide_debug::HideDebug, Config};

#[derive(Clone, Debug)]
pub struct State {
    pub config: HideDebug<Config>,
    pub handlebars: HideDebug<Handlebars<'static>>,
    pub mysql_pool: sqlx::MySqlPool,
    pub sqlite_pool: sqlx::SqlitePool,

    session_cache: Cache<String, Session>,
    user_cache: Cache<String, User>,
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

#[derive(Clone, Debug)]
pub struct Session {
    pub ckey: String,
}

async fn create_mysql_pool(config: &Config) -> color_eyre::Result<sqlx::MySqlPool> {
    let db_pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&config.db_url)
        .await?;

    Ok(db_pool)
}

async fn create_sqlite_pool() -> color_eyre::Result<sqlx::SqlitePool> {
    let db_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("data.db")
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
            sqlite_pool: create_sqlite_pool().await?,

            session_cache: small_cache(),
            user_cache: small_cache(),

            config: HideDebug(config),
        })
    }

    pub fn render_template<T: Serialize>(&self, path: &'static str, data: T) -> Response {
        // #[derive(Serialize)]
        // struct Template<T> {
        //     #[serde(flatten)]
        //     data: T,
        // }

        match self.handlebars.render(path, &data) {
            Ok(response) => Html(response).into_response(),
            Err(error) => {
                tracing::error!("failed to render template {path}: {error:#?}");

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
        session_key: &str,
    ) -> color_eyre::Result<Option<Session>> {
        if let Some(session) = self.session_cache.get(&session_key.to_string()) {
            return Ok(Some(session));
        }

        let session = match sqlx::query("SELECT ckey FROM sessions WHERE session_key = ?")
            .bind(session_key)
            .fetch_optional(&self.sqlite_pool)
            .await
        {
            Ok(Some(session)) => session,
            Ok(None) => {
                tracing::debug!("couldn't find a session from {session_key}");
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

        let ckey = session.get("ckey");

        self.session_cache
            .insert(session_key.to_string(), Session { ckey })
            .await;

        Ok(Some(
            self.session_cache.get(&session_key.to_string()).unwrap(),
        ))
    }

    #[tracing::instrument]
    pub async fn user(self: Arc<Self>, ckey: &str) -> color_eyre::Result<User> {
        if let Some(user) = self.user_cache.get(&ckey.to_string()) {
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
                        .bind(&rank)
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

        Ok(self.user_cache.get(&ckey.to_string()).unwrap())
    }

    /// Creates the session, and returns the session key
    #[tracing::instrument]
    pub async fn create_session_for(self: Arc<Self>, ckey: &str) -> color_eyre::Result<String> {
        match sqlx::query("SELECT session_key FROM sessions WHERE ckey = ?")
            .bind(ckey)
            .fetch_optional(&self.sqlite_pool)
            .await
        {
            Ok(Some(row)) => {
                return Ok(row.get("session_key"));
            }
            Ok(None) => {}
            Err(error) => return Err(error.into()),
        };

        // rand::random() uses thread_rng(), which is cryptographically secure.
        let session_key = format!("{:x}", rand::random::<u128>());

        sqlx::query("INSERT INTO sessions (ckey, session_key) VALUES (?, ?)")
            .bind(&ckey)
            .bind(&session_key)
            .execute(&self.sqlite_pool)
            .await
            .context("error inserting session")?;

        Ok(session_key)
    }
}
