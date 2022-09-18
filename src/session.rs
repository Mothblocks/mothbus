use std::io::Read;

use once_cell::sync::OnceCell;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub ckey: String,
    exp: usize,
}

pub fn new_session_token(ckey: &str) -> color_eyre::Result<String> {
    match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &Session {
            ckey: ckey.to_owned(),
            exp: (chrono::Utc::now() + chrono::Duration::days(365)).timestamp() as usize,
        },
        &jsonwebtoken::EncodingKey::from_secret(get_secret_token()),
    ) {
        Ok(token) => Ok(token),
        Err(error) => {
            tracing::error!("error creating session token: {error:#?}");
            Err(error.into())
        }
    }
}

pub fn session_from_token(token: &str) -> Option<Session> {
    match jsonwebtoken::decode::<Session>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(get_secret_token()),
        &jsonwebtoken::Validation::default(),
    ) {
        Ok(token_data) => Some(token_data.claims),
        Err(error) => {
            tracing::debug!("invalid JWT token\n- token: {token}\n- error: {error:#?}");
            None
        }
    }
}

const SECRET_KEY_FILE: &str = "jwt_secret_key.txt";

pub fn get_secret_token() -> &'static [u8] {
    static SECRET_TOKEN: OnceCell<Vec<u8>> = OnceCell::new();

    SECRET_TOKEN.get_or_init(|| match std::fs::File::open(SECRET_KEY_FILE) {
        Ok(mut file) => {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).unwrap();
            buf
        }

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!("creating new jwt secret key");

            let secret = rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(64)
                .collect();

            std::fs::write(SECRET_KEY_FILE, &secret).expect("can't write jwt secret key");

            secret
        }

        Err(error) => {
            panic!("can't open jwt secret key: {error}");
        }
    })
}
