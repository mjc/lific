//! Label commands and preferences are Rust operations behind fresh cookie gates.
use super::super::{context, session};
use crate::{
    db::models::Role, error::LificError, realtime::RealtimeHub,
    services::project_overview::LabelCommand,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::procedure,
};
type Outcome = (Result<String, String>, usize);
fn read_labels(cx: &Cx, project: i64) -> Result<Vec<crate::db::models::Label>, LificError> {
    let conn = context::db(cx).read()?;
    crate::db::queries::list_labels(&conn, project)
}
// Keep primitive wire arguments separate because expr! does not support tuple literals.
#[allow(clippy::too_many_arguments)]
#[procedure("/__native_overview/label")]
pub(super) async fn mutate(
    cx: &Cx,
    account: i64,
    project: i64,
    command: String,
    id: i64,
    value: String,
    color: String,
    touched: bool,
) -> topcoat::Result<Outcome> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Ok((Err("Your account changed. Reload this page.".into()), 0));
    }
    let result = caller
        .scope(async {
            crate::authz::require_structure_role(context::db(cx), &caller.identity, project)?;
            let labels = read_labels(cx, project)?;
            let name = value.trim_matches(super::labels_model::js_whitespace);
            let action = match command.as_str() {
                "create" | "rename" => {
                    if name.is_empty() {
                        return Ok(labels.len());
                    }
                    let previous = labels.iter().find(|label| label.id == id);
                    if command == "rename" && previous.is_some_and(|label| label.name == name) {
                        return Ok(labels.len());
                    }
                    if super::labels_model::name_taken(
                        &labels,
                        name,
                        if command == "rename" { Some(id) } else { None },
                    ) {
                        return Err(LificError::Conflict(format!(
                            "A label named \"{name}\" already exists."
                        )));
                    }
                    if command == "create" {
                        LabelCommand::Create {
                            name: name.into(),
                            color: if touched {
                                color
                            } else {
                                super::labels_model::color_for_name(name).into()
                            },
                        }
                    } else {
                        LabelCommand::Rename {
                            id,
                            name: name.into(),
                        }
                    }
                }
                "color" => {
                    let color = super::labels_model::normalize_hex(&value).ok_or_else(|| {
                        LificError::BadRequest("Enter a three- or six-digit hex color.".into())
                    })?;
                    if labels
                        .iter()
                        .any(|label| label.id == id && label.color == color)
                    {
                        return Ok(labels.len());
                    }
                    LabelCommand::Recolor { id, color }
                }
                "delete" => LabelCommand::Delete { id },
                "merge" => LabelCommand::Merge {
                    id,
                    into: value.parse().map_err(|_| {
                        LificError::BadRequest("Choose a label to merge into.".into())
                    })?,
                },
                "presets" => {
                    // Same sequential, separately committed starter actions as master.
                    for (name, color) in super::labels_model::PRESETS {
                        if !super::labels_model::name_taken(&labels, name, None) {
                            crate::services::project_overview::label(
                                context::db(cx),
                                app_context::<RealtimeHub>(cx),
                                &caller.identity,
                                project,
                                LabelCommand::Create {
                                    name: name.into(),
                                    color: color.into(),
                                },
                            )?;
                        }
                    }
                    return Ok(read_labels(cx, project)?.len());
                }
                _ => return Err(LificError::BadRequest("Unknown label action.".into())),
            };
            crate::services::project_overview::label(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                project,
                action,
            )?;
            Ok(read_labels(cx, project)?.len())
        })
        .await;
    match result {
        Ok(count) => Ok((Ok("saved".into()), count)),
        Err(error) => Ok((Err(super::actions::error_message(error)), 0)),
    }
}
#[procedure("/__native_overview/label_color")]
pub(super) async fn preview_color(
    cx: &Cx,
    name: String,
    color: String,
    touched: bool,
) -> topcoat::Result<(String, String)> {
    session::read(cx, context::caller(cx))?;
    let color = if touched {
        super::labels_model::safe_color(&color)
    } else {
        let name = name.trim_matches(super::labels_model::js_whitespace);
        super::labels_model::color_for_name(if name.is_empty() { "label" } else { name })
    };
    Ok((color.into(), super::labels_model::color_name(color).into()))
}
#[procedure("/__native_overview/normalize_color")]
pub(super) async fn normalize_color(
    cx: &Cx,
    value: String,
) -> topcoat::Result<Result<String, String>> {
    session::read(cx, context::caller(cx))?;
    Ok(super::labels_model::normalize_hex(&value)
        .ok_or_else(|| "Enter a three- or six-digit hex color.".into()))
}
pub(super) fn stamped_filter(stored: &str, label: &str) -> String {
    // Master spreads the parsed object and changes only filterLabel. Invalid
    // stored JSON falls back to {}, while every unknown object field survives.
    let mut state = serde_json::from_str::<serde_json::Value>(stored)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    state.insert(
        "filterLabel".into(),
        serde_json::Value::String(label.into()),
    );
    serde_json::to_string(&state).expect("JSON object serialization cannot fail")
}
#[procedure("/__native_overview/label_filter")]
pub(super) async fn stamp_filter(
    cx: &Cx,
    account: i64,
    project: i64,
    stored: String,
    label: String,
) -> topcoat::Result<String> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(LificError::Forbidden("Your account changed. Reload this page.".into()).into());
    }
    session::read(
        cx,
        crate::authz::require_role(context::db(cx), &caller.identity, project, Role::Viewer),
    )?;
    Ok(stamped_filter(&stored, &label))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_label_preserves_all_other_project_issue_state_and_recovers_bad_storage() {
        let actual:serde_json::Value=serde_json::from_str(&stamped_filter(r#"{"filterStatus":"todo","filterLabel":"old","searchQuery":"a\\b","sortField":"title","density":"compact","extra":{"foo":true}}"#,"new")).unwrap();
        assert_eq!(
            actual,
            serde_json::json!({"filterStatus":"todo","filterLabel":"new","searchQuery":"a\\b","sortField":"title","density":"compact","extra":{"foo":true}})
        );
        for bad in ["", "invalid", "null", "[]"] {
            assert_eq!(stamped_filter(bad, "bug"), r#"{"filterLabel":"bug"}"#);
        }
    }
}
