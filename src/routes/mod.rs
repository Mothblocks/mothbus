use std::borrow::Cow;

use serde::Serialize;

pub mod errors;
pub use errors::not_found;

mod index;
pub use index::index;

pub mod login;

mod logout;
pub use logout::logout;

mod recent_test_merges;
pub use recent_test_merges::recent_test_merges;

pub mod polls;

pub mod tickets;

pub mod user;

#[cfg(feature = "secret-ban-evasion")]
pub mod evasion;

use crate::state::User;

#[derive(Serialize)]
pub struct TemplateBase {
    pub title: Cow<'static, str>,
    pub user: Option<User>,
}
