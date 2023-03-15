use std::sync::Arc;

use axum::{extract::Query, response::IntoResponse, Extension, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RoundInfoQuery {
    pub round_id: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RoundInfoResult {
    RoundNotFound {
        success: bool,
        error: String,
    },

    RoundInfo {
        success: bool,
        #[serde(flatten)]
        round_info: RoundInfo,
        server: Option<&'static str>,
    },
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RoundInfo {
    pub id: i32,
    pub initialize_datetime: chrono::NaiveDateTime,
    pub end_datetime: Option<chrono::NaiveDateTime>,
    pub server_port: u16,
}

#[tracing::instrument]
pub async fn round_info(
    Extension(state): Extension<Arc<crate::State>>,
    Query(params): Query<RoundInfoQuery>,
) -> impl IntoResponse {
    let Some(round_id) = params.round_id else {
		return Json(RoundInfoResult::RoundNotFound {
			success: false,
			error: "you must specify a round id by passing a `round_id` parameter".to_string(),
		}).into_response();
	};

    let round_info = match sqlx::query_as::<_, RoundInfo>(
        "SELECT id, initialize_datetime, end_datetime, server_port FROM round WHERE id = ?",
    )
    .bind(round_id)
    .fetch_optional(&state.mysql_pool)
    .await
    {
        Ok(Some(round_info)) => round_info,

        Ok(None) => {
            return Json(RoundInfoResult::RoundNotFound {
                success: false,
                error: "round not found".to_string(),
            })
            .into_response();
        }

        Err(error) => {
            tracing::error!("failed to find round info: {error:#}");
            return Json(RoundInfoResult::RoundNotFound {
                success: false,
                error: format!("database error: {error}"),
            })
            .into_response();
        }
    };

    if round_info.end_datetime.is_none() {
        return Json(RoundInfoResult::RoundNotFound {
            success: false,
            error: "round isn't over yet".to_string(),
        })
        .into_response();
    }

    let server = crate::servers::server_by_port(round_info.server_port);

    Json(RoundInfoResult::RoundInfo {
        success: true,
        round_info,
        server: server.map(|server| server.name),
    })
    .into_response()
}
