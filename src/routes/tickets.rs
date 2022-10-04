use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query},
    response::IntoResponse,
    Extension,
};
use color_eyre::eyre::Context;
use http::StatusCode;
use serde::{Deserialize, Serialize, Serializer};

use crate::{auth::AuthenticatedUser, servers::Servers, state::User, State};

use super::{
    errors::{make_forbidden, make_not_found},
    TemplateBase,
};

const SELECT_TICKETS_TEMPLATE: &str = r#"
    SELECT
        first_tickets.*,
        (SELECT
                COUNT(*)
            FROM
                ticket
            WHERE
                ticket.round_id = first_tickets.round_id
                    AND ticket.ticket = first_tickets.ticket) AS conversation_count,
        (SELECT
                action
            FROM
                ticket
            WHERE
                ticket.round_id = first_tickets.round_id
                    AND ticket.ticket = first_tickets.ticket
            ORDER BY id DESC
            LIMIT 1) AS final_response
    FROM
        ticket
            INNER JOIN
        ticket AS first_tickets ON first_tickets.round_id = ticket.round_id
            AND first_tickets.ticket = ticket.ticket
            AND first_tickets.action = 'Ticket Opened'
"#;

#[derive(Serialize)]
struct TicketsTemplate {
    base: TemplateBase,
    can_read_tickets: bool,
    servers: &'static Servers,
}

#[derive(Debug, Deserialize)]
pub struct TicketsParams {
    page: Option<u32>,
    embed: Option<String>,
}

fn render_tickets(
    state: Arc<State>,
    params: TicketsParams,
    tickets_list_template: TicketsListTemplate,
) -> impl IntoResponse {
    if tickets_list_template.tickets.is_empty() {
        return (StatusCode::NO_CONTENT).into_response();
    }

    state
        .render_template(
            if params.embed.is_some() {
                "tickets_list"
            } else {
                "tickets_list_page"
            },
            tickets_list_template,
        )
        .into_response()
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Ticket {
    id: u64,
    action: String,
    #[serde(serialize_with = "decode_html_entities")]
    message: String,
    recipient: Option<String>,
    sender: Option<String>,
    urgent: bool,

    conversation_count: i64,
    round_id: u64,
    ticket: u64,
    final_response: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
struct TicketMessage {
    id: u64,
    action: String,
    #[serde(serialize_with = "decode_html_entities")]
    message: String,
    recipient: Option<String>,
    sender: Option<String>,
    urgent: bool,

    timestamp: chrono::NaiveDateTime,
}

fn decode_html_entities<S: Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&html_escape::decode_html_entities(text))
}

#[tracing::instrument]
pub async fn index(
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> impl IntoResponse {
    state.render_template(
        "tickets",
        TicketsTemplate {
            can_read_tickets: user.can_read_tickets(),
            servers: &crate::servers::SERVERS,

            base: TemplateBase {
                title: "tickets".into(),
                user: Some(user),
            },
        },
    )
}

const TICKETS_PER_PAGE: u32 = 20;

#[derive(Serialize)]
struct TicketsListTemplate {
    base: TemplateBase,
    who: String,
    page: u32,
    tickets: Vec<WithColor<Ticket>>,
}

#[derive(Serialize)]
struct WithColor<T: Serialize> {
    #[serde(flatten)]
    data: T,
    color: String,
}

#[tracing::instrument]
pub async fn for_ckey(
    Path(ckey): Path<String>,
    Query(params): Query<TicketsParams>,
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> impl IntoResponse {
    if !can_read_tickets_for(&user, &ckey) {
        return make_forbidden(
            state,
            &format!(
                "You do not have permission to read the tickets of {}",
                &ckey
            ),
        )
        .await
        .into_response();
    }

    let page = params.page.unwrap_or(1);

    let tickets = match sqlx::query_as::<_, Ticket>(&format!(
        r#"
            {SELECT_TICKETS_TEMPLATE}
            WHERE
                ticket.recipient = ?
                    OR ticket.sender = ?
            GROUP BY ticket.round_id , ticket.ticket
            ORDER BY id DESC
            LIMIT ? OFFSET ?
        "#
    ))
    .bind(&ckey)
    .bind(&ckey)
    .bind(TICKETS_PER_PAGE)
    .bind(page.saturating_sub(1) * TICKETS_PER_PAGE)
    .fetch_all(&state.mysql_pool)
    .await
    .context("failed to fetch tickets")
    {
        Ok(tickets) => tickets
            .into_iter()
            .map(|ticket| WithColor {
                color: match (
                    ticket.sender.as_ref() == Some(&ckey),
                    ticket.recipient.is_some(),
                ) {
                    // We're sender, has recipient - We are sending a ticket
                    (true, true) => "this-admin-to-player".into(),

                    // We're sender, to all admins - This is just a normal ahelp
                    (true, false) => "player".into(),

                    // We are not the sender, so this must be someone else's ahelp
                    (false, _) => "player-ahelping".into(),
                },

                data: ticket,
            })
            .collect(),
        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    render_tickets(
        state,
        params,
        TicketsListTemplate {
            base: TemplateBase {
                title: format!("tickets - {ckey}").into(),
                user: Some(user),
            },

            who: ckey,
            page,
            tickets,
        },
    )
    .into_response()
}

#[tracing::instrument]
pub async fn for_server(
    Path(server_name): Path<String>,
    Query(params): Query<TicketsParams>,
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> impl IntoResponse {
    if !user.can_read_tickets() {
        return make_forbidden(
            state,
            "You do not have permission to read a server's tickets",
        )
        .await
        .into_response();
    }

    let server = match crate::servers::server_by_name(server_name.as_str()) {
        Some(server) => server,
        None => {
            return make_not_found(state, &format!("\"{server_name}\" is not a valid server"))
                .await
                .into_response();
        }
    };

    let page = params.page.unwrap_or(1);

    let tickets = match sqlx::query_as::<_, Ticket>(&format!(
        r#"
            {SELECT_TICKETS_TEMPLATE}
            WHERE
                first_tickets.server_port = ?
                AND first_tickets.action = 'Ticket Opened'
            GROUP BY id
            ORDER BY id DESC
            LIMIT ? OFFSET ?
        "#,
    ))
    .bind(server.port)
    .bind(TICKETS_PER_PAGE)
    .bind(page.saturating_sub(1) * TICKETS_PER_PAGE)
    .fetch_all(&state.mysql_pool)
    .await
    .context("failed to fetch tickets for server")
    {
        Ok(tickets) => tickets
            .into_iter()
            .map(|ticket| WithColor {
                color: if ticket.recipient.is_some() {
                    "admin1".into()
                } else {
                    "player-ahelping".into()
                },

                data: ticket,
            })
            .collect(),

        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    render_tickets(
        state,
        params,
        TicketsListTemplate {
            base: TemplateBase {
                title: format!("tickets - {server_name}").into(),
                user: Some(user),
            },
            who: server_name,
            page,
            tickets,
        },
    )
    .into_response()
}

#[tracing::instrument]
pub async fn for_round(
    Path(round_id): Path<u64>,
    Query(params): Query<TicketsParams>,
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> impl IntoResponse {
    if !user.can_read_tickets() {
        return make_forbidden(
            state,
            "You do not have permission to read a round's tickets.",
        )
        .await
        .into_response();
    }

    let page = params.page.unwrap_or(1);

    let tickets = match sqlx::query_as::<_, Ticket>(&format!(
        r#"
            {SELECT_TICKETS_TEMPLATE}
            WHERE
                first_tickets.round_id = ?
                AND first_tickets.action = 'Ticket Opened'
            GROUP BY first_tickets.id
            ORDER BY id ASC
            LIMIT ? OFFSET ?
        "#,
    ))
    .bind(round_id)
    .bind(TICKETS_PER_PAGE)
    .bind(page.saturating_sub(1) * TICKETS_PER_PAGE)
    .fetch_all(&state.mysql_pool)
    .await
    .context("failed to fetch tickets for round")
    {
        Ok(tickets) => tickets
            .into_iter()
            .map(|ticket| WithColor {
                color: if ticket.recipient.is_some() {
                    "admin1".into()
                } else {
                    "player-ahelping".into()
                },

                data: ticket,
            })
            .collect(),

        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    render_tickets(
        state,
        params,
        TicketsListTemplate {
            base: TemplateBase {
                title: format!("tickets - round {round_id}").into(),
                user: Some(user),
            },
            who: format!("round {round_id}"),
            page,
            tickets,
        },
    )
    .into_response()
}

#[derive(Serialize)]
struct TicketTemplate {
    base: TemplateBase,
    ticket_messages: Vec<WithColor<TicketMessage>>,
    round_id: u64,
    ticket_no: u64,
}

#[tracing::instrument]
pub async fn for_ticket(
    Path((round_id, ticket)): Path<(u64, u64)>,
    Extension(state): Extension<Arc<State>>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> impl IntoResponse {
    const FORBIDDEN_TICKET: &str = "You are not allowed to read this ticket.";

    let ticket_messages: Vec<_> = match sqlx::query_as::<_, TicketMessage>(
        r#"
            SELECT
                *
            FROM
                ticket
            WHERE
                round_id = ? AND ticket = ?
            ORDER BY id
        "#,
    )
    .bind(round_id)
    .bind(ticket)
    .fetch_all(&state.mysql_pool)
    .await
    .context("failed to fetch ticket")
    {
        Ok(ticket_messages) => {
            let player_ckey = ticket_messages
                .first()
                .map(|ticket_message| {
                    ticket_message
                        .recipient
                        .as_ref()
                        .or(ticket_message.sender.as_ref())
                        .cloned()
                })
                .unwrap_or_default();

            let mut ckey_to_color: HashMap<String, String> = HashMap::new();
            let mut admin_count = 0;

            ticket_messages
                .into_iter()
                .map(|ticket_message| WithColor {
                    color: match &ticket_message.sender {
                        Some(sender) => ckey_to_color
                            .entry(sender.to_owned())
                            .or_insert_with(|| {
                                if Some(sender) == player_ckey.as_ref() {
                                    "player".to_owned()
                                } else {
                                    admin_count += 1;
                                    format!("admin{admin_count}")
                                }
                            })
                            .clone(),

                        None => "system".to_owned(),
                    },

                    data: ticket_message,
                })
                .collect()
        }
        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    if ticket_messages.is_empty() {
        if user.can_read_tickets() {
            return make_not_found(state, "Ticket not found")
                .await
                .into_response();
        }

        // Lie to unauthorized users so we don't leak information about a round with status codes
        return make_forbidden(state, FORBIDDEN_TICKET)
            .await
            .into_response();
    }

    if let Some(sender) = &ticket_messages[0].data.sender {
        if !can_read_tickets_for(&user, sender) {
            return make_forbidden(state, FORBIDDEN_TICKET)
                .await
                .into_response();
        }
    } else if !user.can_read_tickets() {
        return make_forbidden(state, FORBIDDEN_TICKET)
            .await
            .into_response();
    }

    state.render_template(
        "ticket",
        TicketTemplate {
            base: TemplateBase {
                title: format!("ticket #{round_id}/{ticket}").into(),
                user: Some(user),
            },

            ticket_messages,
            round_id,
            ticket_no: ticket,
        },
    )
}

fn can_read_tickets_for(user: &User, ckey: &str) -> bool {
    user.can_read_tickets() || user.ckey == ckey
}
