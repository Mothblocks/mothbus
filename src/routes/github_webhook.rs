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

pub(self) fn simplify_body(input: &str) -> String {
    let comments_regex = regex::Regex::new(r"(?s)<!--(.*?)-->").unwrap();
    let mut output = comments_regex.replace_all(input, "").to_string();

    let issue_summary_regex = regex::Regex::new(r"(?sm)## Issue Summary:.*?(^.+)").unwrap();
    if let Some(captures) = issue_summary_regex.captures(&output) {
        output = captures.get(1).unwrap().as_str().to_string();
    } else {
        let reproduction_regex = regex::Regex::new(r"(?sm)## Reproduction:.*?(^.+)").unwrap();
        if let Some(captures) = reproduction_regex.captures(&output) {
            output = captures.get(1).unwrap().as_str().to_string();
        }
    }

    let headers_regex = regex::Regex::new(r"(?m)^#+\s*(.*?)\s*$").unwrap();
    output = headers_regex.replace_all(&output, "**$1**").to_string();

    let images_regex = regex::Regex::new(r"(?s)!\[.*?\]\(.*?\)").unwrap();
    output = images_regex.replace_all(&output, "").to_string();

    let mut lines = output.lines().collect::<Vec<_>>();
    lines.retain(|x| !x.is_empty());

    if lines.len() > 4 {
        lines.truncate(4);
        lines.push("...");
    }

    output = lines.join("\n");

    output.trim().to_owned()
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

            let mut body =
                simplify_body(webhook_body["issue"]["body"].as_str().unwrap_or_default());

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

    #[test]
    fn simplify_body_headers() {
        assert_eq!(simplify_body("# Header\nText"), "**Header**\nText")
    }

    #[test]
    fn simplify_body_keep() {
        assert_eq!(
            simplify_body("**bold** *italics* __underline__ **__bold and underline__**"),
            "**bold** *italics* __underline__ **__bold and underline__**"
        )
    }

    #[test]
    fn simplify_body_comments() {
        assert_eq!(simplify_body("<!-- comment\nwith new lines -->"), "")
    }

    #[test]
    fn simplify_images() {
        assert_eq!(simplify_body("![image](https://google.com)"), "");
    }

    #[test]
    fn simplify_template() {
        let simplified_template = simplify_body(indoc::indoc! {"
        Reporting client version: 515.1620

        <!-- Write **BELOW** The Headers and **ABOVE** The comments else it may not be viewable -->
        ## Round ID:
        [219951](https://scrubby.melonmesa.com/round/219951)
        <!--- **INCLUDE THE ROUND ID**
        If you discovered this issue from playing tgstation hosted servers:
        [Round ID]: # (It can be found in the Status panel or retrieved from https://sb.atlantaned.space/rounds ! The round id let's us look up valuable information and logs for the round the bug happened.)-->
        
        <!-- If you are reporting an issue found in another branch or codebase, you MUST link the branch or codebase repo in your issue report or it will be closed. For branches, If you have not pushed your code up, you must either reproduce it on master or push your code up before making an issue report. For other codebases, if you do not have a public code repo you will be refused help unless you can completely reproduce the issue on our code. -->
        
        ## Testmerges:
        - [Attack chain refactoring: Broadening `tool_act` into `item_interact`, moving some item interactions to... `atom/item_interact` / `item/interact_with_atom`](https://www.github.com/tgstation/tgstation/pull/79968)
        - [Changes Virology Rather Than Killing It](https://www.github.com/tgstation/tgstation/pull/79854)
        <!-- If you're certain the issue is to be caused by a test merge [OOC tab -> Show Server Revision], report it in the pull request's comment section rather than on the tracker(If you're unsure you can refer to the issue number by prefixing said number with #. The issue number can be found beside the title after submitting it to the tracker).If no testmerges are active, feel free to remove this section. -->
        
        ## Reproduction:
        ![image](https://github.com/tgstation/tgstation/assets/76874615/b4a0d0a0-9ae7-4b46-a9a0-84478b3d980c)
        By rightclicking an egg into the soup pot, it both drops its reagents into the pot And gets added to the pot as an ingredient
        you can pull the egg out but it just vanishes
        <!-- Explain your issue in detail, including the steps to reproduce it. Issues without proper reproduction steps or explanation are open to being ignored/closed by maintainers.-->
        
        <!-- **For Admins:** Oddities induced by var-edits and other admin tools are not necessarily bugs. Verify that your issues occur under regular circumstances before reporting them. -->
        
        <!-- If you are reporting a runtime error you must include the runtime in your report or your report will be closed. -->
        "});

        assert!(
            simplified_template.starts_with("By rightclicking"),
            "Template doesn't start at reproduction:\n{simplified_template}",
        );
    }
}
