use std::io::Read;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub db_url: String,
    pub port: u16,

    #[serde(default)]
    pub mock_login: bool,

    pub oauth2: OAuth2Options,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OAuth2Options {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl Config {
    pub fn read_from_file() -> color_eyre::Result<Self> {
        let mut file = std::fs::File::open("config.toml")?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}
