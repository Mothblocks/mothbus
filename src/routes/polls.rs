use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{extract::Path, response::IntoResponse, Extension};
use serde::Serialize;
use sqlx::{mysql::MySqlRow, Row};

use crate::{auth::AuthenticatedUserOptional, reserved_cache::ReservedCache, state::User, State};

use super::TemplateBase;

#[derive(Clone, Debug, Serialize)]
pub struct Poll {
    id: i32,

    question: String,
    subtitle: Option<String>,
    options: Vec<(String, i64)>,

    admin_only: bool,
    created_by_ckey: String,
    seconds_until_end: i64,
    wait_for_results: bool,
}

impl Poll {
    pub fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,

            question: row.try_get("question")?,
            subtitle: row.try_get("subtitle")?,
            options: vec![(row.try_get("text")?, row.try_get("vote_count")?)],

            admin_only: row.try_get("adminonly")?,
            created_by_ckey: row.try_get("createdby_ckey")?,
            seconds_until_end: row.try_get("seconds_until_end")?,
            wait_for_results: !row.try_get("dontshow")?,
        })
    }

    pub fn merge_with(&mut self, other: &MySqlRow) -> Result<(), sqlx::Error> {
        assert_eq!(self.id, other.try_get::<i32, _>("id")?);
        self.options
            .push((other.try_get("text")?, other.try_get("vote_count")?));
        Ok(())
    }
}

#[tracing::instrument]
pub async fn create_poll_cache(state: Arc<State>) -> color_eyre::Result<HashMap<i32, Poll>> {
    let rows = sqlx::query(
        "SELECT 
            poll_question.id,
            poll_question.question,
            poll_question.subtitle,
            UNIX_TIMESTAMP(poll_question.endtime) - UNIX_TIMESTAMP() AS seconds_until_end,
            poll_question.adminonly,
            poll_question.dontshow,
            poll_question.createdby_ckey,
            poll_option.text,
            (SELECT 
                    COUNT(*)
                FROM
                    poll_vote
                WHERE
                    poll_vote.optionid = poll_option.id) AS vote_count
        FROM
            poll_question
                JOIN
            poll_option ON poll_option.pollid = poll_question.id
                AND poll_option.deleted = FALSE
        WHERE
            (poll_question.polltype = 'OPTION'
                OR poll_question.polltype = 'MULTICHOICE')
                AND poll_question.deleted = FALSE
        ORDER BY id DESC
    ",
    )
    .fetch_all(&state.mysql_pool)
    .await?;

    let mut polls: HashMap<i32, Poll> = HashMap::new();

    for row in rows {
        let id = row.try_get("id")?;

        if let Some(poll) = polls.get_mut(&id) {
            poll.merge_with(&row)?;
        } else {
            polls.insert(id, Poll::from_row(&row)?);
        }
    }

    Ok(polls)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PollAccessDenyReason {
    AdminOnly,
    NotFinished,
}

fn try_access_poll(user: Option<&User>, poll: &Poll) -> Result<(), PollAccessDenyReason> {
    let user_can_read_admin_polls = match &user {
        Some(user) => user.can_read_admin_only_polls(),
        None => false,
    };

    if poll.admin_only && !user_can_read_admin_polls {
        return Err(PollAccessDenyReason::AdminOnly);
    }

    if poll.seconds_until_end > 0 && poll.wait_for_results {
        return Err(PollAccessDenyReason::NotFinished);
    }

    Ok(())
}

#[derive(Serialize)]
struct PollsTemplate {
    base: TemplateBase,
    polls: Vec<Poll>,
    can_read_admin_only_polls: bool,
}

#[tracing::instrument]
pub async fn index(
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUserOptional(user): AuthenticatedUserOptional,
) -> impl IntoResponse {
    let PollCache(cache) = &state.poll_cache;

    let polls = match Arc::clone(cache).get(Arc::clone(&state)).await {
        Ok(polls) => polls,
        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    tracing::trace!("number of polls = {}", polls.len());
    tracing::trace!("first poll: {:#?}", polls.values().next());

    let mut polls = polls.values().cloned().collect::<Vec<_>>();

    polls.retain(|poll| try_access_poll(user.as_ref(), poll).is_ok());

    polls.sort_by_key(|poll| -poll.id);

    state.render_template(
        "polls",
        PollsTemplate {
            can_read_admin_only_polls: match &user {
                Some(user) => user.can_read_admin_only_polls(),
                None => false,
            },

            base: TemplateBase {
                title: "polls".into(),
                user,
            },

            polls,
        },
    )
}

#[derive(Serialize)]
struct PollTemplate {
    base: TemplateBase,
    poll: Poll,
}

#[tracing::instrument]
pub async fn for_poll(
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUserOptional(user): AuthenticatedUserOptional,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let PollCache(cache) = &state.poll_cache;

    let polls = match Arc::clone(cache).get(Arc::clone(&state)).await {
        Ok(polls) => polls,
        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    let poll = match polls.get(&id) {
        Some(poll) => poll,
        None => {
            return super::errors::make_not_found(state, "poll not found")
                .await
                .into_response();
        }
    };

    if let Err(problem) = try_access_poll(user.as_ref(), poll) {
        return super::errors::make_forbidden(
            state,
            match problem {
                PollAccessDenyReason::AdminOnly => "you must be an admin",
                PollAccessDenyReason::NotFinished => "poll has not finished yet",
            },
        )
        .await
        .into_response();
    }

    state.render_template(
        "poll",
        PollTemplate {
            base: TemplateBase {
                title: poll.question.clone().into(),
                user,
            },

            poll: poll.clone(),
        },
    )
}

#[derive(Debug)]
pub struct PollCache(Arc<ReservedCache<HashMap<i32, crate::routes::polls::Poll>>>);

impl PollCache {
    pub fn new() -> Self {
        Self(Arc::new(ReservedCache::new(
            Duration::from_secs(60),
            create_poll_cache,
        )))
    }
}

impl Default for PollCache {
    fn default() -> Self {
        Self::new()
    }
}
