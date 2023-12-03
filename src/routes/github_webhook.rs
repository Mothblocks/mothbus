use std::sync::Arc;

use axum::{response::IntoResponse, Extension};
use hmac::{Hmac, Mac};
use http::StatusCode;

fn bytes_to_hex_display(input: &[u8]) -> String {
    let mut bytes = String::new();

    for byte in input {
        bytes.push_str(&format!("{:02x}", byte));
    }

    bytes
}

pub(self) fn verify_signature(secret: &str, body: &[u8], signature: &str) -> Result<(), String> {
    let mut hmac: Hmac<sha2::Sha256> =
        Hmac::new_from_slice(secret.as_bytes()).expect("failed to create hmac");
    hmac.update(body);

    let signature = signature.replace("sha256=", "");
    let finalized = bytes_to_hex_display(&hmac.finalize().into_bytes()[..]);

    if finalized == signature {
        Ok(())
    } else {
        Err(finalized)
    }
}

#[tracing::instrument(skip(body))]
pub async fn github_webhook(
    Extension(state): Extension<Arc<crate::State>>,
    headers: axum::http::header::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let Some(signature) = headers.get("X-Hub-Signature-256") else {
		return (StatusCode::BAD_REQUEST, "missing signature").into_response();
	};

    let signature = signature.to_str().unwrap_or_default();
    if let Err(calculated) = verify_signature(
        &state.config.github_webhook.secret,
        body.as_ref(),
        signature,
    ) {
        tracing::debug!("invalid signature: {signature}, expected {calculated}");
        return (StatusCode::BAD_REQUEST, "invalid signature").into_response();
    }

    let event_type = headers
        .get("X-GitHub-Event")
        .map(|x| x.to_str().unwrap_or_default())
        .unwrap_or_default();

    tracing::debug!("received github webhook event {event_type}");

    if event_type == "issues" {
        let webhook_body: serde_json::Value =
            serde_json::from_slice(body.as_ref()).unwrap_or_default();

        if webhook_body.get("action").map(|x| x.as_str()) == Some(Some("opened")) {
            tracing::debug!("received new issue");

            let mut body = webhook_body["issue"]["body"]
                .as_str()
                .unwrap_or_default()
                .to_owned();

            if body.len() > 500 {
                body.truncate(500);
                body.push_str("...");
            }

            let mut title = format!(
                "[{}] Issue opened: #{} {}",
                webhook_body["repository"]["full_name"]
                    .as_str()
                    .unwrap_or_default(),
                webhook_body["issue"]["number"],
                webhook_body["issue"]["title"].as_str().unwrap_or_default(),
            );

            if title.bytes().len() > 256 {
                tracing::debug!("title too long, truncating");
                title.truncate(250);
                title.push_str("...");
            }

            if let Err(error) = reqwest::Client::new()
                .post(&state.config.github_webhook.discord_url)
                .json(&serde_json::json!({
                    "embeds": [{
                        "title": title,

                        "description": body,

                        "url": webhook_body["issue"]["html_url"],

                        "author": {
                            "name": webhook_body["issue"]["user"]["login"],
                            "icon_url": webhook_body["issue"]["user"]["avatar_url"],
                        }
                    }],
                }))
                .send()
                .await
            {
                tracing::error!("failed to send discord webhook: {error:#}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to send discord webhook",
                )
                    .into_response();
            }
        }
    }

    (StatusCode::NO_CONTENT, "").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_signature_simple() {
        assert_eq!(
            verify_signature(
                "It's a Secret to Everybody",
                b"Hello, World!",
                "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
            ),
            Ok(())
        );
    }
}
