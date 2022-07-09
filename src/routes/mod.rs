use std::borrow::Cow;

use serde::Serialize;

mod index;
pub use index::index;

#[derive(Serialize)]
pub struct TemplateBase {
    pub title: Cow<'static, str>,
}
