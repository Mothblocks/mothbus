use std::borrow::Cow;

use serde::Serialize;

pub mod errors;
pub use errors::not_found;

mod index;
pub use index::index;

pub mod login;

pub mod tickets;

use crate::state::User;

#[derive(Serialize)]
pub struct TemplateBase {
    pub title: Cow<'static, str>,
    pub user: Option<User>,
}
