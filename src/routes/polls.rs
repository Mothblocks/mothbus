use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use axum::{extract::Path, response::IntoResponse, Extension};
use moka::future::Cache;
use serde::Serialize;
use sqlx::{mysql::MySqlRow, Row};

use crate::{auth::AuthenticatedUserOptional, state::User, State};

use super::TemplateBase;

#[derive(Clone, Debug, Serialize)]
pub enum PollOptions {
    Choice(Vec<(String, i64)>),
    NumVal(Vec<(String, Vec<(i32, i32)>)>),
    Text(Vec<TextAnswer>),
}

#[derive(Clone, Debug, Serialize)]
pub struct TextAnswer {
    ckey: String,
    text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Poll {
    id: i32,

    question: String,
    subtitle: Option<String>,
    options: PollOptions,

    admin_only: bool,
    created_by_ckey: String,
    start_date: String,
    wait_for_results: bool,

    seconds_until_end: i64,
}

impl Poll {
    fn from_row(row: &MySqlRow, options: PollOptions) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,

            question: row.try_get("question")?,
            subtitle: row.try_get("subtitle")?,
            options,

            admin_only: row.try_get("adminonly")?,
            created_by_ckey: row.try_get("createdby_ckey")?,
            start_date: row.try_get("start_date")?,
            wait_for_results: !row.try_get("dontshow")?,

            seconds_until_end: row.try_get("seconds_until_end")?,
        })
    }

    pub fn from_choiced_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Self::from_row(
            row,
            PollOptions::Choice(vec![(row.try_get("text")?, row.try_get("vote_count")?)]),
        )
    }

    pub fn from_numval_row(row: &MySqlRow, votes: BTreeMap<i32, i32>) -> Result<Self, sqlx::Error> {
        Self::from_row(
            row,
            PollOptions::NumVal(vec![(row.try_get("text")?, votes.into_iter().collect())]),
        )
    }

    pub fn from_text_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Self::from_row(
            row,
            PollOptions::Text(vec![TextAnswer {
                ckey: row.try_get("ckey")?,
                text: row.try_get("replytext")?,
            }]),
        )
    }

    pub fn merge_with_choiced(&mut self, other: &MySqlRow) -> Result<(), sqlx::Error> {
        assert_eq!(self.id, other.try_get::<i32, _>("id")?);

        let PollOptions::Choice(choice_votes) = &mut self.options else {
            panic!("expected choice poll");
        };

        choice_votes.push((other.try_get("text")?, other.try_get("vote_count")?));

        Ok(())
    }

    pub fn merge_with_numval(
        &mut self,
        other: &MySqlRow,
        votes: BTreeMap<i32, i32>,
    ) -> Result<(), sqlx::Error> {
        assert_eq!(self.id, other.try_get::<i32, _>("id")?);

        let PollOptions::NumVal(numval_votes) = &mut self.options else {
            panic!("expected numval poll");
        };

        numval_votes.push((other.try_get("text")?, votes.into_iter().collect()));

        Ok(())
    }

    pub fn merge_with_text(
        &mut self,
        other_poll: &MySqlRow,
        other_answer: &MySqlRow,
    ) -> Result<(), sqlx::Error> {
        assert_eq!(self.id, other_poll.try_get::<i32, _>("id")?);

        let PollOptions::Text(text_answers) = &mut self.options else {
            panic!("expected text poll");
        };

        text_answers.push(TextAnswer {
            ckey: other_answer.try_get("ckey")?,
            text: other_answer.try_get("replytext")?,
        });

        Ok(())
    }
}

#[tracing::instrument]
pub async fn create_poll_cache(state: Arc<State>) -> color_eyre::Result<HashMap<i32, Poll>> {
    tracing::trace!("creating poll cache");

    let rows = sqlx::query(
        "SELECT 
            poll_question.id,
            poll_option.id AS option_id,
            poll_question.question,
            poll_question.subtitle,
            DATE_FORMAT(poll_question.starttime, '%Y-%m-%d') AS start_date,
            UNIX_TIMESTAMP(poll_question.endtime) - UNIX_TIMESTAMP() AS seconds_until_end,
            poll_question.adminonly,
            poll_question.dontshow,
            poll_question.createdby_ckey,
            poll_option.text,
            poll_question.polltype,
            poll_option.minval,
            poll_option.maxval,
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
                OR poll_question.polltype = 'MULTICHOICE'
                OR poll_question.polltype = 'NUMVAL')
                AND poll_question.deleted = FALSE
        ORDER BY id DESC
    ",
    )
    .fetch_all(&state.mysql_pool)
    .await?;

    let text_polls_and_answers = sqlx::query(
        "SELECT 
            poll_question.id,
            poll_question.question,
            poll_question.subtitle,
            DATE_FORMAT(poll_question.starttime, '%Y-%m-%d') AS start_date,
            UNIX_TIMESTAMP(poll_question.endtime) - UNIX_TIMESTAMP() AS seconds_until_end,
            poll_question.adminonly,
            poll_question.dontshow,
            poll_question.createdby_ckey,
            poll_textreply.ckey,
            poll_textreply.replytext
        FROM
            poll_question
                JOIN
            poll_textreply ON poll_textreply.pollid = poll_question.id
        WHERE
            polltype = 'TEXT'
                AND poll_question.deleted = FALSE
                AND poll_textreply.deleted = FALSE
        ORDER BY poll_question.id DESC",
    )
    .fetch_all(&state.mysql_pool)
    .await?;

    let numval_votes = sqlx::query(
        "SELECT 
            optionid, rating, COUNT(*) AS vote_count
        FROM
            poll_vote
        WHERE
            rating IS NOT NULL
        GROUP BY optionid, rating
        ORDER BY id DESC",
    )
    .fetch_all(&state.mysql_pool)
    .await?;

    let mut polls: HashMap<i32, Poll> = HashMap::new();

    for row in rows {
        let id = row.try_get("id")?;

        if row.try_get::<String, _>("polltype")? == "NUMVAL" {
            let min: i32 = row.try_get("minval")?;
            let max: i32 = row.try_get("maxval")?;

            let mut votes = BTreeMap::new();

            for i in min..=max {
                votes.insert(i, 0);
            }

            let option_id: i32 = row.try_get("option_id")?;

            for vote in &numval_votes {
                if vote.try_get::<i32, _>("optionid")? == option_id {
                    votes.insert(vote.try_get("rating")?, vote.try_get("vote_count")?);
                }
            }

            if let Some(poll) = polls.get_mut(&id) {
                poll.merge_with_numval(&row, votes)?;
            } else {
                polls.insert(id, Poll::from_numval_row(&row, votes)?);
            }
        } else if let Some(poll) = polls.get_mut(&id) {
            poll.merge_with_choiced(&row)?;
        } else {
            polls.insert(id, Poll::from_choiced_row(&row)?);
        }
    }

    for row in text_polls_and_answers {
        let id = row.try_get("id")?;
        if let Some(poll) = polls.get_mut(&id) {
            poll.merge_with_text(&row, &row)?;
        } else {
            polls.insert(id, Poll::from_text_row(&row)?);
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
    let cache = &state.poll_cache;

    let polls = match cache.get(Arc::clone(&state)).await {
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
    can_read_text_ckeys: bool,
}

#[tracing::instrument]
pub async fn for_poll(
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUserOptional(user): AuthenticatedUserOptional,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let cache = &state.poll_cache;

    let polls = match cache.get(Arc::clone(&state)).await {
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
            poll: poll.clone(),
            can_read_text_ckeys: match &user {
                Some(user) => user.can_read_text_ckeys(),
                None => false,
            },

            base: TemplateBase {
                title: poll.question.clone().into(),
                user,
            },
        },
    )
}

type Polls = Arc<HashMap<i32, Poll>>;

#[derive(Debug)]
pub struct PollCache {
    cache: Cache<(), Polls>,
}

impl PollCache {
    pub fn new() -> Self {
        Self {
            cache: Cache::builder()
                .time_to_live(Duration::from_secs(60))
                .build(),
        }
    }

    async fn get(&self, state: Arc<State>) -> color_eyre::Result<Polls> {
        self.cache
            .try_get_with((), async move {
                match create_poll_cache(state).await {
                    Ok(polls) => Ok(Arc::new(polls)),
                    Err(error) => Err(error),
                }
            })
            .await
            .map_err(|error| {
                Arc::try_unwrap(error)
                    .unwrap_or_else(|arc| color_eyre::Report::msg(arc.to_string()))
            })
    }
}

impl Default for PollCache {
    fn default() -> Self {
        Self::new()
    }
}
