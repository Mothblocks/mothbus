use axum::response::{Html, IntoResponse};
use color_eyre::eyre::Context;
use handlebars::Handlebars;
use http::StatusCode;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct State {
    pub handlebars: Handlebars<'static>,
}

impl State {
    pub fn new() -> color_eyre::Result<Self> {
        let mut handlebars = Handlebars::new();
        handlebars.set_dev_mode(true);

        for template in std::fs::read_dir("dist")? {
            let template = template?;

            if template.path().extension() != Some("html".as_ref()) {
                continue;
            }

            handlebars
                .register_template_file(
                    &template
                        .path()
                        .file_stem()
                        .expect("no file stem")
                        .to_string_lossy(),
                    template.path(),
                )
                .context("failed to register template")?;
        }

        Ok(Self { handlebars })
    }

    pub fn render_template<T: Serialize>(&self, path: &'static str, data: T) -> impl IntoResponse {
        #[derive(Serialize)]
        struct Template<T> {
            #[serde(flatten)]
            data: T,
            parent: &'static str,
        }

        match self.handlebars.render(
            path,
            &Template {
                data,
                parent: "base",
            },
        ) {
            Ok(response) => Html(response).into_response(),
            Err(error) => {
                tracing::error!("failed to render template {path}: {error:#?}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to render template. this is a bug.\n{error}"),
                )
                    .into_response()
            }
        }
    }
}
