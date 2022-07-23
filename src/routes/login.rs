use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, Path, Query},
    response::{AppendHeaders, IntoResponse, Redirect},
    Extension,
};
use color_eyre::eyre::Context;
use http::{header::SET_COOKIE, StatusCode};
use serde::Deserialize;

use crate::config::OAuth2Options;

#[tracing::instrument]
pub async fn index(Extension(state): Extension<Arc<crate::State>>) -> impl IntoResponse {
    // TODO: &state URL parameter
    Redirect::to(&format!(
		"https://tgstation13.org/phpBB/app.php/tgapi/oauth/auth?response_type=code&client_id={}&redirect_uri={}",
		state.config.oauth2.client_id,
		state.config.oauth2.redirect_uri,
	))
}

#[derive(Deserialize)]
pub struct OAuthQuery {
    code: String,
}

// TODO: ?error=access_denied&errordesc=The+user+did+not+authorize+the+request+or+there+was+an+error+processing+their+authorization.
#[tracing::instrument]
pub async fn oauth(
    Extension(state): Extension<Arc<crate::State>>,
    Query(OAuthQuery { code }): Query<OAuthQuery>,
) -> impl IntoResponse {
    let ckey = match ckey_for_auth(&code, &state.config.oauth2).await {
        Ok(Some(ckey)) => ckey,

        Ok(None) => {
            return super::errors::make_unauthorized(state, "forum account has no ckey")
                .await
                .into_response();
        }

        Err(error) => {
            return super::errors::make_internal_server_error(state, error)
                .await
                .into_response();
        }
    };

    tracing::debug!("logged in as {}", ckey);

    login_as(state, &ckey).await.into_response()
}

async fn ckey_for_auth(
    code: &str,
    oauth_config: &OAuth2Options,
) -> color_eyre::Result<Option<String>> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum AuthorizationCodeResponse {
        Success {
            access_token: String,
        },

        Error {
            error: String,
            error_description: String,
        },
    }

    let client = reqwest::ClientBuilder::new()
        .user_agent("moth.fans")
        .build()
        .context("couldn't create http client")?;

    tracing::debug!("requesting access token for {code}");

    let authorization_token_response_text = client
        .post("https://tgstation13.org/phpBB/app.php/tgapi/oauth/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &oauth_config.client_id),
            ("client_secret", &oauth_config.client_secret),
            ("redirect_uri", &oauth_config.redirect_uri),
        ])
        .send()
        .await
        .context("error sending request to oauth server")?
        .text()
        .await
        .context("error getting response text for authorization token")?;

    let access_token = match serde_json::from_str(&authorization_token_response_text) {
        Ok(AuthorizationCodeResponse::Success { access_token }) => access_token,

        Ok(AuthorizationCodeResponse::Error {
            error,
            error_description,
        }) => {
            tracing::error!("authorization response failure: {error_description} ({error})");

            return Err(color_eyre::eyre::eyre!(
                "authorization response failure: {error_description} ({error})"
            ));
        }

        Err(error) => {
            tracing::error!(
                "invalid authorization token response body: {authorization_token_response_text}"
            );

            return Err(error).context("error parsing authorization token response");
        }
    };

    #[derive(Deserialize)]
    struct UserResponse {
        byond_ckey: Option<String>,
    }

    tracing::debug!("requesting user info for {access_token}");

    let tgapi_user_response_text = client
        .get("https://tgstation13.org/phpBB/app.php/tgapi/user/me")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .context("error sending request to oauth server")?
        .text()
        .await
        .context("error getting response text for user")?;

    let user_response: UserResponse = match serde_json::from_str(&tgapi_user_response_text) {
        Ok(user_response) => user_response,
        Err(error) => {
            tracing::error!("invalid user response body: {tgapi_user_response_text}");

            return Err(error).context("error parsing user response");
        }
    };

    Ok(user_response.byond_ckey)
}

#[tracing::instrument]
pub async fn mock_login(
    Extension(state): Extension<Arc<crate::State>>,
    Path(ckey): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !state.config.mock_login {
        return (StatusCode::FORBIDDEN, "mock logins are disabled").into_response();
    }

    // Extra security
    if !addr.ip().is_loopback() {
        return (
            StatusCode::FORBIDDEN,
            "mock logins are only allowed on localhost",
        )
            .into_response();
    }

    login_as(state, &ckey).await.into_response()
}

async fn login_as(state: Arc<crate::State>, ckey: &str) -> impl IntoResponse {
    let session_key = match state.create_session_for(ckey).await {
        Ok(session_key) => session_key,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };

    (
        AppendHeaders([(
            SET_COOKIE,
            format!("session_key={session_key}; SameSite=None; Secure; Path=/; Max-Age=31536000",),
        )]),
        Redirect::temporary("/"),
    )
        .into_response()
}
