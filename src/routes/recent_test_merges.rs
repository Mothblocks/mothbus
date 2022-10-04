use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{response::IntoResponse, Extension, Json};
use http::StatusCode;
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::Row;
use tokio::sync::RwLock;

#[derive(Clone, Serialize)]
pub struct TestMerge {
    pub round_id: u64,
    pub datetime: chrono::NaiveDateTime,
    pub test_merges: Vec<u64>,
    pub server: String,
    pub url: String,
}

type TestMerges = Vec<TestMerge>;
type RecentTestMergesCache = Option<(Instant, TestMerges)>;

static LAST_RECENT_TEST_MERGES: Lazy<Arc<RwLock<RecentTestMergesCache>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

#[tracing::instrument]
pub async fn recent_test_merges(
    Extension(state): Extension<Arc<crate::State>>,
) -> impl IntoResponse {
    {
        let last_recent_test_merges = LAST_RECENT_TEST_MERGES.read().await;
        if let Some((last_update, test_merges)) = last_recent_test_merges.as_ref() {
            if last_update.elapsed() < Duration::from_secs(60 * 10) {
                return Json(test_merges.clone()).into_response();
            }
        }
    }

    let mut last_recent_test_merges = LAST_RECENT_TEST_MERGES.write().await;

    tracing::debug!("requesting recent test merges from db");

    match sqlx::query(
        "SELECT 
			round_id,
			datetime,
			JSON_EXTRACT(json, '$.data.*.number') AS test_merges,
			round.server_port
		FROM
			tgstation13.feedback
		JOIN round ON round.id = round_id
		WHERE
			key_name = 'testmerged_prs'
		ORDER BY round.id DESC
		LIMIT 200;
	",
    )
    .fetch_all(&state.mysql_pool)
    .await
    {
        Ok(rows) => {
            let output = rows.into_iter()
                .map(|row| {
                    let datetime: chrono::NaiveDateTime = row.get::<_, &'static str>("datetime");

                    let test_merges: Vec<u64> = serde_json::from_str::<HashSet<String>>(
                        &row.get::<String, &'static str>("test_merges"),
                    )
                    .expect("test_merges is not a valid json array")
                    .into_iter()
                    .map(|test_merge_id| {
                        test_merge_id.parse().unwrap_or_else(|_| {
                            panic!("{test_merge_id} is not a valid test_merge_id")
                        })
                    })
                    .collect();

                    let port = row.get("server_port");
                    let round_id = row.get("round_id");

					let server_name = crate::servers::server_by_port(port)
						.map(|server| server.name.to_owned())
						.unwrap_or_else(|| format!("Unknown ({port})"));

                    TestMerge {
                        round_id,
						datetime,
                        test_merges,
                        server: server_name.clone(),
                        url: format!(
                            "https://tgstation13.org/parsed-logs/{server_name}/data/logs/{}/{}/{}/round-{round_id}/",
							datetime.format("%Y"),
							datetime.format("%m"),
							datetime.format("%d"),
                        ),
                    }
                })
                .collect::<Vec<_>>();

            tracing::debug!("Updating recent test merges");

            *last_recent_test_merges = Some((Instant::now(), output.clone()));

            Json(output).into_response()
        }

        Err(error) => {
            tracing::error!("failed to fetch new test merges: {error:#?}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{error}")).into_response()
        }
    }
}
