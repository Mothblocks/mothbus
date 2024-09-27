use crate::state::User;

use std::sync::Arc;

use axum::{
    async_trait,
    extract::{FromRequest, RequestParts},
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use http::StatusCode;

use crate::{session::Session, State};

async fn get_session<B: Send>(
    request: &mut RequestParts<B>,
) -> color_eyre::Result<Option<Session>> {
    let Extension(state) = Extension::<Arc<State>>::from_request(request)
        .await
        .expect("can't get state");

    let cookie_jar = axum_extra::extract::cookie::CookieJar::from_request(request)
        .await
        .unwrap();

    let session_jwt = match cookie_jar.get("session_jwt") {
        Some(session_jwt) => session_jwt.value(),
        None => return Ok(None),
    };

    state.session(session_jwt).await
}

async fn get_user<B: Send>(request: &mut RequestParts<B>) -> color_eyre::Result<Option<User>> {
    let Extension(state) = Extension::<Arc<State>>::from_request(request)
        .await
        .expect("can't get state");

    let session = match get_session(request).await? {
        Some(session) => session,
        None => return Ok(None),
    };

    Ok(Some(state.user(&session.ckey).await?))
}

pub struct AuthenticatedUserOptional(pub Option<User>);

#[async_trait]
impl<B: Send> FromRequest<B> for AuthenticatedUserOptional {
    type Rejection = (StatusCode, String);

    async fn from_request(request: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        match get_user(request).await {
            Ok(user_optional) => Ok(Self(user_optional)),
            Err(error) => {
                tracing::error!("error getting user (in AuthenticatedUserOptional): {error:#?}");

                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("error getting user, this is a bug. please report to mothblocks"),
                ))
            }
        }
    }
}

pub struct AuthenticatedUser(pub User);

#[async_trait]
impl<B: Send> FromRequest<B> for AuthenticatedUser {
    type Rejection = Response;

    async fn from_request(request: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        match get_user(request).await {
            Ok(Some(user)) => Ok(Self(user)),

            Ok(None) => {
                // TODO: Include a page to go back to
                tracing::debug!("user not authenticated in AuthenticatedUser, redirecting");

                Err(Redirect::temporary("/login").into_response())
            }

            Err(error) => {
                tracing::error!("error getting user (in AuthenticatedUser): {error:#?}");

                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("error getting user, this is a bug. please report to mothblocks"),
                )
                    .into_response())
            }
        }
    }
}
