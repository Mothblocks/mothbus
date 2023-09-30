use handlebars::{Helper, PathAndJson, RenderError};
use serde::Deserialize;

mod english_duration;
pub use english_duration::EnglishDuration;

pub(crate) fn read_param<'de, T: Deserialize<'de>>(
    path_and_json: &'de PathAndJson,
) -> Result<T, RenderError> {
    T::deserialize(path_and_json.value()).map_err(|error| {
        RenderError::from_error(
            &format!(
                "can't deserialize parameter `{}`",
                path_and_json
                    .relative_path()
                    .map(|x| x.as_str())
                    .unwrap_or_default()
            ),
            error,
        )
    })
}

pub(crate) fn require_param<'de, T: Deserialize<'de>>(
    helper: &'de Helper,
    index: usize,
    name: &str,
) -> Result<T, RenderError> {
    match helper.param(index) {
        Some(path_and_json) => read_param(path_and_json),
        None => Err(RenderError::new(format!("{name} is required"))),
    }
}
