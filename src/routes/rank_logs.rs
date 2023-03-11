use std::sync::Arc;

use axum::{extract::Query, response::IntoResponse, Extension};
use color_eyre::eyre::Context;
use serde::{Deserialize, Serialize};

use crate::{auth::AuthenticatedUserOptional, State};

use super::TemplateBase;

const LOGS_PER_PAGE: u32 = 50;

#[derive(Serialize)]
struct RankLogsTemplate {
    base: TemplateBase,
    logs: Vec<LogEntry>,
    page: u32,
}

#[derive(Debug, Deserialize)]
pub struct RankLogsParams {
    page: Option<u32>,
    embed: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct LogEntry {
    datetime: chrono::NaiveDateTime,
    adminckey: String,
    target: String,
    operation: String,
    log: String,
}

#[tracing::instrument]
pub async fn rank_logs(
    Query(params): Query<RankLogsParams>,
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUserOptional(user): AuthenticatedUserOptional,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);

    state.render_template(
        if params.embed.is_some() {
            "rank_logs_list"
        } else {
            "rank_logs"
        },
        RankLogsTemplate {
            logs: match admin_logs(page, &state.mysql_pool).await {
                Ok(logs) => logs,
                Err(error) => {
                    return super::errors::make_internal_server_error(state, error)
                        .await
                        .into_response();
                }
            },
            page,
            base: TemplateBase {
                title: "admin rank logs".into(),
                user,
            },
        },
    )
}

async fn admin_logs(page: u32, mysql_pool: &sqlx::MySqlPool) -> color_eyre::Result<Vec<LogEntry>> {
    let rows = sqlx::query_as::<_, LogEntry>(
    	"SELECT datetime, adminckey, target, operation, log FROM admin_log ORDER BY datetime DESC LIMIT ? OFFSET ?",
    )
		.bind(LOGS_PER_PAGE)
		.bind(page.saturating_sub(1) * LOGS_PER_PAGE)
		.fetch_all(mysql_pool)
		.await
		.context("error fetching admin logs")?;

    Ok(rows)
}
