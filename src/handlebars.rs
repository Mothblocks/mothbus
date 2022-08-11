use color_eyre::eyre::Context;
use handlebars::{Handlebars, Helper, HelperDef, PathAndJson, RenderContext, RenderError};
use serde::Deserialize;

use crate::state::User;

fn read_param<'de, T: Deserialize<'de>>(path_and_json: &'de PathAndJson) -> Result<T, RenderError> {
    T::deserialize(path_and_json.value())
        .map_err(|error| RenderError::from_error("can't deserialize parameter", error))
}

fn require_param<'de, T: Deserialize<'de>>(
    helper: &'de Helper,
    index: usize,
    name: &str,
) -> Result<T, RenderError> {
    match helper.param(index) {
        Some(path_and_json) => read_param(path_and_json),
        None => Err(RenderError::new(format!("{name} is required"))),
    }
}

struct UserReadsTickets;

impl HelperDef for UserReadsTickets {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'reg, 'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc handlebars::Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<handlebars::ScopedJson<'reg, 'rc>, RenderError> {
        let user: User = require_param(helper, 0, "user")?;

        Ok(handlebars::ScopedJson::from(serde_json::Value::Bool(
            user.can_read_tickets(),
        )))
    }
}

pub fn create_handlebars() -> color_eyre::Result<Handlebars<'static>> {
    let mut handlebars = Handlebars::new();
    handlebars.set_dev_mode(true);

    handlebars.register_helper("user_reads_tickets", Box::new(UserReadsTickets));

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

    Ok(handlebars)
}
