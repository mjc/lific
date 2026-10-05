//! Pinned ProjectSettings overview decisions. Browser clock is display input only.

use chrono::{DateTime, NaiveDateTime};

use crate::db::models::{Activity, Issue, Priority, Project, Role, Status, UpdateProject};

pub(crate) const GROUP_WARNING: &str =
    "Project created, but it wasn't added to the selected group. You can add it from the sidebar.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Capabilities {
    pub(crate) edit: bool,
    pub(crate) manage: bool,
    pub(crate) publish: bool,
}

pub(crate) fn capabilities(
    enforced: bool,
    admin: bool,
    role: Option<Role>,
    primary_lead: bool,
) -> Capabilities {
    Capabilities {
        edit: !enforced || admin || matches!(role, Some(Role::Maintainer | Role::Lead)),
        manage: !enforced || admin || role == Some(Role::Lead),
        publish: admin || role == Some(Role::Lead) || primary_lead,
    }
}

pub(crate) fn days_since(time: &str, now_milliseconds: i64) -> i64 {
    let parsed = DateTime::parse_from_rfc3339(&format!("{time}Z"))
        .map(|date| date.timestamp_millis())
        .or_else(|_| {
            NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S%.f")
                .map(|date| date.and_utc().timestamp_millis())
        });
    parsed.map_or(0, |then| {
        now_milliseconds
            .saturating_sub(then)
            .div_euclid(86_400_000)
            .max(0)
    })
}

pub(crate) fn importance(issue: &Issue, now_milliseconds: i64) -> f64 {
    let weight = match issue.priority {
        Priority::Urgent => 100.0,
        Priority::High => 55.0,
        Priority::Medium => 25.0,
        Priority::Low => 10.0,
        Priority::None => 4.0,
    };
    let multiplier = match issue.status {
        Status::Todo => 1.25,
        Status::Active => 1.15,
        _ => 1.0,
    };
    (weight
        + days_since(&issue.created_at, now_milliseconds) as f64 * 0.5
        + days_since(&issue.updated_at, now_milliseconds) as f64 * 0.6)
        * multiplier
}

pub(crate) struct Attention<'a> {
    pub(crate) issues: Vec<&'a Issue>,
    pub(crate) more: usize,
}

pub(crate) fn attention(issues: &[Issue], now_milliseconds: i64) -> Attention<'_> {
    let mut open: Vec<_> = issues
        .iter()
        .filter(|issue| {
            matches!(
                issue.status,
                Status::Backlog | Status::Todo | Status::Active
            )
        })
        .collect();
    // sort_by is stable, like modern JS Array.sort: equal scores retain server list order.
    open.sort_by(|a, b| {
        importance(b, now_milliseconds).total_cmp(&importance(a, now_milliseconds))
    });
    let more = open.len().saturating_sub(6);
    open.truncate(6);
    Attention { issues: open, more }
}

pub(crate) fn age_label(days: i64) -> String {
    if days >= 60 {
        format!("{}mo", (days + 15) / 30)
    } else if days >= 1 {
        format!("{days}d")
    } else {
        "today".into()
    }
}

pub(crate) fn actor_name(activity: &Activity) -> &str {
    activity
        .actor_display_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            activity
                .actor_username
                .as_deref()
                .filter(|name| !name.is_empty())
        })
        .unwrap_or(if activity.actor_is_bot {
            "a bot"
        } else {
            "system"
        })
}

pub(crate) fn activity_text(activity: &Activity) -> String {
    let verb = match activity.action.as_str() {
        "create" => "created",
        "delete" => "deleted",
        "update" => "updated",
        other => other,
    };
    match activity
        .entity_label
        .as_deref()
        .filter(|label| !label.is_empty())
    {
        Some(label) => format!("{verb} {} {label}", activity.entity_type),
        None => format!("{verb} {}", activity.entity_type),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Field {
    Name,
    Description,
    Emoji,
    Identifier,
}

pub(crate) fn field_patch(field: Field, draft: &str, saved: &Project) -> Option<UpdateProject> {
    // String.trim's ECMAScript whitespace includes FEFF and excludes U+0085.
    let trim = |value: &str| {
        value.trim_matches(|c: char| matches!(c, '\u{0009}'..='\u{000D}' | '\u{0020}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}')).to_owned()
    };
    let mut patch = UpdateProject::default();
    match field {
        Field::Name => {
            let value = trim(draft);
            if value.is_empty() || value == saved.name {
                return None;
            }
            patch.name = Some(value);
        }
        Field::Description => {
            let value = trim(draft);
            if value == saved.description {
                return None;
            }
            patch.description = Some(value);
        }
        Field::Emoji => patch.emoji = Some((!draft.is_empty()).then(|| draft.to_owned())),
        Field::Identifier => {
            let value = trim(draft).to_uppercase();
            if value.is_empty() || value == saved.identifier {
                return None;
            }
            patch.identifier = Some(value);
        }
    }
    Some(patch)
}

pub(crate) fn notice_message(detail: Option<String>, group_warning: bool) -> Option<String> {
    detail
        .filter(|detail| !detail.is_empty())
        .or_else(|| group_warning.then(|| GROUP_WARNING.to_owned()))
}

pub(crate) fn query_notice(query: &str) -> Option<&str> {
    let mut values = query
        .split('&')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(key, _)| *key == "notice");
    let (_, value) = values.next()?;
    if values.next().is_some()
        || value.len() != 48
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(value)
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
