use handlebars::{Handlebars, Helper, HelperDef, RenderContext, RenderError};

fn pluralize<'a>(count: i64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

pub struct EnglishDuration;

impl HelperDef for EnglishDuration {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'reg, 'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc handlebars::Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<handlebars::ScopedJson<'reg, 'rc>, RenderError> {
        let other_datetime: chrono::NaiveDateTime =
            super::require_param(helper, 0, "other_datetime")?;
        let now = chrono::Utc::now().naive_utc();

        let duration = now - other_datetime;

        Ok(handlebars::ScopedJson::from(serde_json::Value::String(
            if duration.num_days() > 0 {
                format!(
                    "{} {} ago",
                    duration.num_days(),
                    pluralize(duration.num_days(), "day", "days")
                )
            } else if duration.num_hours() > 0 {
                format!(
                    "{} {} ago",
                    duration.num_hours(),
                    pluralize(duration.num_hours(), "hour", "hours")
                )
            } else if duration.num_minutes() > 0 {
                format!(
                    "{} {} ago",
                    duration.num_minutes(),
                    pluralize(duration.num_minutes(), "minute", "minutes")
                )
            } else {
                format!(
                    "{} {} ago",
                    duration.num_seconds(),
                    pluralize(duration.num_seconds(), "second", "seconds")
                )
            },
        )))
    }
}
