//! Native commands reuse the canonical HTTP mutation policies in-process.

use axum::{Extension, Json, extract::State};
use topcoat::{context::Cx, runtime::procedure};

use super::super::context;
use crate::error::LificError;

#[procedure("/__native_instance_settings/save_text")]
pub(super) async fn save_text(
    cx: &Cx,
    account: i64,
    field: String,
    value: String,
) -> topcoat::Result<(bool, String)> {
    let caller = match context::caller(cx) {
        Ok(caller) => caller,
        Err(error) => return Ok((false, error.to_string())),
    };
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) if user.id == account => user,
        Ok(_) => return Ok((false, "Your account changed. Reload this page.".into())),
        Err(error) => return Ok((false, error.to_string())),
    };
    if !user.is_admin {
        return Ok((false, "only an admin can do this".into()));
    }
    let patch = match field.as_str() {
        "name" => crate::api::auth::InstanceSettingsPatchReq {
            instance_name: Some(value),
            ..Default::default()
        },
        _ => return Ok((false, "unknown instance setting".into())),
    };
    let headers = match caller.session_headers() {
        Ok(headers) => headers,
        Err(error) => return Ok((false, error.to_string())),
    };
    let result = caller
        .scope(crate::api::auth::instance_settings_patch(
            State(context::db(cx).clone()),
            Extension(topcoat::context::app_context::<crate::realtime::RealtimeHub>(cx).clone()),
            Extension(caller.identity.clone()),
            None,
            headers,
            Json(patch),
        ))
        .await;
    Ok(match result {
        Ok(settings) => (
            true,
            settings.0["instance_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ),
        Err(LificError::BadRequest(message) | LificError::Forbidden(message)) => (false, message),
        Err(error) => {
            tracing::error!(error=%error, "native instance setting update failed");
            (
                false,
                "Couldn't save that instance setting. Try again.".into(),
            )
        }
    })
}
