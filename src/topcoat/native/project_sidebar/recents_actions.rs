//! The recents read pipeline shares the sidebar catalog, never its authority.
use super::{
    actions, model,
    recents_model::{Focus, Row, Section, State, Token},
};
use crate::{
    error::LificError,
    server::topcoat_frontend::native::context,
    services::project_recents::{self, ReadFailure},
};
use topcoat::{context::Cx, runtime::procedure};

pub(super) type Published = (
    String,
    String,
    String,
    i64,
    bool,
    bool,
    String,
    bool,
    String,
    String,
);
pub(super) type Rows = (i64, Option<(i64, String)>, Option<Section>, Vec<Row>);

fn parse<T: serde::de::DeserializeOwned>(wire: &str) -> Result<T, LificError> {
    if wire.len() > 256 * 1024 {
        return Err(LificError::BadRequest(
            "Recent state is too large. Reload this page.".into(),
        ));
    }
    serde_json::from_str(wire)
        .map_err(|_| LificError::BadRequest("Recent state changed. Reload this page.".into()))
}
fn state(wire: &str, account: i64) -> Result<State, LificError> {
    let state: State = parse(wire)?;
    if state.owner != account || !state.valid_envelope() {
        return Err(LificError::Forbidden(
            "Your recent project changed. Reload this page.".into(),
        ));
    }
    Ok(state)
}
pub(super) fn selected<'a>(catalog: &'a model::State, path: &str) -> Option<&'a model::Project> {
    let identifier = crate::server::topcoat_frontend::shell::ParsedRoute::parse(path).project?;
    catalog
        .catalog
        .projects
        .iter()
        .find(|project| project.identifier.eq_ignore_ascii_case(identifier))
}
pub(super) fn published(state: &State, focus: String) -> Result<Published, LificError> {
    let rows: Rows = (
        state.owner,
        state.project.clone(),
        state.section,
        state.rows().to_vec(),
    );
    Ok((
        actions::encode(state)?,
        actions::encode(&rows)?,
        state.section.map_or_else(String::new, |section| {
            format!("Recent {}", section.as_str())
        }),
        state.project.as_ref().map_or(0, |project| project.0),
        state.visible,
        state.loading,
        state.error.clone().unwrap_or_default(),
        state.open,
        focus,
        state.status(),
    ))
}
fn authorize(cx: &Cx, account: i64, project: i64) -> Result<context::Caller, LificError> {
    let caller = context::caller(cx)?;
    let user = crate::api::require_user(&caller.identity)?;
    if user.id != account {
        return Err(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        ));
    }
    let conn = context::db(cx).read()?;
    let fresh = crate::auth::fresh_caller(&conn, account)?;
    let identity = Some(crate::auth::fresh_identity(
        &fresh,
        crate::actor::Transport::Web,
    ));
    if fresh.is_bot {
        return Err(LificError::Forbidden(
            "A signed-in browser account is required.".into(),
        ));
    }
    crate::authz::require_role_conn(&conn, &identity, project, crate::db::models::Role::Viewer)?;
    Ok(caller)
}

#[procedure("/__native_sidebar/recents_prepare")]
pub(super) async fn prepare(
    cx: &Cx,
    account: i64,
    cache: String,
    catalog: String,
    path: String,
    open: bool,
) -> topcoat::Result<(Published, String)> {
    let mut state = state(&cache, account)?;
    state.open = open;
    let (_, catalog) = actions::decoded(cx, account, &catalog)?;
    let project = selected(&catalog, &path);
    let section = project.and_then(|project| Section::for_path(&project.identifier, &path));
    let token = state.begin(account, project, section, true);
    Ok((
        published(&state, String::new())?,
        token
            .map(|token| actions::encode(&token))
            .transpose()?
            .unwrap_or_default(),
    ))
}

#[procedure("/__native_sidebar/recents_read")]
pub(super) async fn read(cx: &Cx, account: i64, token: String) -> topcoat::Result<String> {
    let token: Token = parse(&token)?;
    let result = if token.owner != account {
        Err(ReadFailure::from(LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )))
    } else {
        authorize(cx, account, token.project.0)
            .map_err(ReadFailure::from)
            .and_then(|caller| {
                project_recents::load(
                    context::db(cx),
                    &caller.identity,
                    account,
                    token.project.0,
                    token.section,
                )
            })
    };
    Ok(actions::encode(&result)?)
}

#[procedure("/__native_sidebar/recents_finish")]
pub(super) async fn finish(
    cx: &Cx,
    account: i64,
    cache: String,
    token: String,
    result: String,
    focused: String,
) -> topcoat::Result<Published> {
    Ok(complete(cx, account, &cache, &token, &result, &focused)?)
}

fn complete(
    cx: &Cx,
    account: i64,
    cache: &str,
    token: &str,
    result: &str,
    focused: &str,
) -> Result<Published, LificError> {
    let mut state = state(cache, account)?;
    let token: Token = parse(token)?;
    let result: Result<Vec<Row>, ReadFailure> = match authorize(cx, account, token.project.0) {
        Ok(_) => parse(result)?,
        Err(error) => Err(error.into()),
    };
    let focus = state.finish(token, result, (!focused.is_empty()).then_some(focused));
    if !state.valid_envelope() {
        return Err(LificError::BadRequest(
            "Recent result changed. Reload this page.".into(),
        ));
    }
    let target = match focus {
        Some(Focus::Keep(href)) => href,
        Some(Focus::Disclosure) => "disclosure".into(),
        _ => String::new(),
    };
    let decision = actions::encode(&(target, focused))?;
    published(&state, decision)
}

#[procedure("/__native_sidebar/recents_failed")]
pub(super) async fn failed(
    cx: &Cx,
    account: i64,
    cache: String,
    token: String,
    focused: String,
) -> topcoat::Result<Published> {
    let result: Result<Vec<Row>, ReadFailure> = Err(ReadFailure {
        access: false,
        message: "Could not load recent items. Try again.".into(),
    });
    Ok(complete(
        cx,
        account,
        &cache,
        &token,
        &actions::encode(&result)?,
        &focused,
    )?)
}

/// Reauthorize cache rows independently on every shard render. Submitted
/// catalog fields and hrefs never make an inaccessible project visible.
pub(super) fn rows(cx: &Cx, account: i64, wire: &str) -> Result<Vec<Row>, LificError> {
    let (owner, project, section, rows): Rows = parse(wire)?;
    if owner != account || rows.len() > 5 {
        return Err(LificError::Forbidden("Recent owner changed.".into()));
    }
    let (Some((project, identifier)), Some(section)) = (project, section) else {
        return Ok(Vec::new());
    };
    authorize(cx, account, project)?;
    let conn = context::db(cx).read()?;
    let fresh = crate::db::queries::get_project(&conn, project)?;
    if fresh.identifier != identifier {
        return Ok(Vec::new());
    }
    let project = model::Project {
        id: fresh.id,
        identifier: fresh.identifier,
        name: fresh.name,
        emoji: fresh.emoji,
    };
    let mut state = State::new(account, false);
    let token = state
        .begin(account, Some(&project), Some(section), true)
        .ok_or_else(|| LificError::Forbidden("Recent project changed.".into()))?;
    state.finish(token, Ok(rows), None);
    if !state.valid_envelope() {
        return Err(LificError::BadRequest("Recent links changed.".into()));
    }
    Ok(state.rows().to_vec())
}
