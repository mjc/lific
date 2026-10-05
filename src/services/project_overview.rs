//! Authorized reads for the pinned master project overview.

use crate::{
    authz,
    db::{
        DbPool,
        models::{
            Activity, AuthUser, Issue, IssueStatusCounts, Label, ListIssuesQuery, MemberWithUser,
            Page, Project, ProjectGroup, Role,
        },
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug)]
pub(crate) struct OverviewReads {
    pub(crate) user: AuthUser,
    pub(crate) projects: Vec<Project>,
    pub(crate) project: Project,
    pub(crate) role: Option<Role>,
    pub(crate) enforced: bool,
    pub(crate) counts: Result<IssueStatusCounts, LificError>,
    pub(crate) issues: Result<Vec<Issue>, LificError>,
    pub(crate) activity: Result<Vec<Activity>, LificError>,
    pub(crate) labels: Result<Vec<Label>, LificError>,
    pub(crate) pages: Result<Vec<Page>, LificError>,
    pub(crate) members: Result<Vec<MemberWithUser>, LificError>,
    pub(crate) groups: Result<Vec<ProjectGroup>, LificError>,
}

pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
) -> Result<OverviewReads, LificError> {
    let caller = crate::api::require_user(identity)?;
    let (user, projects, project, role, settings, counts, issues, activity, labels, pages, members) = {
        let conn = db.read()?;
        let tx = conn.unchecked_transaction()?;
        let fresh = crate::auth::fresh_caller(&tx, caller.id)?;
        let current = Some(crate::auth::fresh_identity(
            &fresh,
            identity
                .as_ref()
                .map_or(crate::actor::Transport::Web, |i| i.transport),
        ));
        let user = crate::api::require_user(&current)?;
        let visible = super::projects::sidebar_visibility(&tx, user.id)?;
        let projects = super::projects::normalize_sidebar_ranks(authz::filter_visible(
            queries::list_projects_for_user(&tx, user.id)?,
            &visible,
            |p| Some(p.id),
        ));
        // Master searches its visible catalog using exact string equality.
        let project = projects
            .iter()
            .find(|project| project.identifier == identifier)
            .cloned()
            .ok_or_else(|| LificError::NotFound(format!("Project {identifier} not found")))?;
        authz::require_role_conn(&tx, &current, project.id, Role::Viewer)?;
        let role = queries::members::get_member_role(&tx, project.id, user.id)?;
        let settings = queries::settings::get(&tx)?;
        let counts = queries::count_issues_by_status(&tx, project.id);
        let issues = queries::list_issues(
            &tx,
            &ListIssuesQuery {
                project_id: Some(project.id),
                limit: Some(1000),
                ..Default::default()
            },
        )
        .map(|mut issues| {
            queries::retain_visible_relations(&tx, &mut issues, visible.as_ref());
            issues
        });
        let activity = super::activity::list_activity_conn(
            &tx,
            &current,
            queries::activity::ActivityScope::Project(project.id),
            None,
            Some(14),
            Some(0),
        )
        .map(|feed| feed.items);
        let labels = queries::list_labels(&tx, project.id);
        let pages = queries::list_pages(
            &tx,
            Some(project.id),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let members = queries::members::list_members_with_users(&tx, project.id);
        tx.commit()?;
        (
            user, projects, project, role, settings, counts, issues, activity, labels, pages,
            members,
        )
    };
    // Existing shared policy owns caller-personal group filtering.
    let groups = super::project_form::list_groups(db, identity);
    Ok(OverviewReads {
        user,
        projects,
        project,
        role,
        enforced: settings.authz_enforced,
        counts,
        issues,
        activity,
        labels,
        pages,
        members,
        groups,
    })
}

#[cfg(test)]
#[path = "project_overview_tests.rs"]
mod tests;

/// Shared extraction candidate for REST's existing update handler.
/// Transport adapters resolve credentials, scope the actor, then call once.
pub(crate) fn update(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    session_token: Option<&str>,
    project_id: i64,
    input: crate::db::models::UpdateProject,
) -> Result<Project, LificError> {
    authz::require_role(db, identity, project_id, Role::Lead)?;
    let caller = crate::api::require_user(identity)?;
    let grants_lead = matches!(input.lead_user_id, Some(Some(_)));
    if grants_lead && session_token.is_none() {
        return Err(LificError::Forbidden(
            "recent authentication required".into(),
        ));
    }
    let project = db.transaction(|tx| {
        let fresh = if grants_lead {
            crate::auth::revalidate_recent_session(
                tx,
                session_token.ok_or_else(|| {
                    LificError::Forbidden("recent authentication required".into())
                })?,
                caller.id,
            )?
        } else {
            crate::auth::fresh_caller(tx, caller.id)?
        };
        let current = Some(crate::auth::fresh_identity(
            &fresh,
            crate::actor::Transport::Web,
        ));
        authz::require_role_conn(tx, &current, project_id, Role::Lead)?;
        queries::update_project(tx, project_id, &input)
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectUpdated {
        project_id: project.id,
    });
    Ok(project)
}

/// Existing delete policy and former-member audience retained in one commit.
pub(crate) fn delete(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<(), LificError> {
    delete_with_confirmation(db, realtime, identity, project_id, None)
}

/// Reject a browser confirmation invalidated by a concurrent project rekey.
pub(crate) fn delete_confirmed(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    expected_identifier: &str,
) -> Result<(), LificError> {
    delete_with_confirmation(
        db,
        realtime,
        identity,
        project_id,
        Some(expected_identifier),
    )
}

fn delete_with_confirmation(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    expected_identifier: Option<&str>,
) -> Result<(), LificError> {
    authz::require_project_delete_role(db, identity, project_id)?;
    let caller = crate::api::require_user(identity)?;
    let (project, audience) = db.transaction(|tx| {
        let fresh = crate::auth::fresh_caller(tx, caller.id)?;
        let current = Some(crate::auth::fresh_identity(
            &fresh,
            crate::actor::Transport::Web,
        ));
        authz::require_project_delete_role_conn(tx, &current, project_id)?;
        if let Some(expected) = expected_identifier {
            // The immediate write transaction serializes this admission check
            // with both project rekeys and the destructive cascade below.
            let saved = queries::get_project(tx, project_id)?;
            if saved.identifier != expected {
                return Err(LificError::BadRequest(
                    "Type the project's exact identifier to confirm.".into(),
                ));
            }
        }
        queries::delete_project_with_audience(tx, project_id)
    })?;
    let event = crate::realtime::RealtimeEvent::ProjectDeleted {
        project_id: project.id,
    };
    match audience {
        Some(users) => realtime.send_to_users(event, users),
        None => realtime.send(event),
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum LabelCommand {
    Create { name: String, color: String },
    Rename { id: i64, name: String },
    Recolor { id: i64, color: String },
    Delete { id: i64 },
    Merge { id: i64, into: i64 },
}

/// Candidate shared label writer: a native adapter never invokes a REST request.
/// REST resource handlers must delegate to this same policy before integration.
pub(crate) fn label(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    command: LabelCommand,
) -> Result<(), LificError> {
    authz::require_structure_role(db, identity, project_id)?;
    let caller = crate::api::require_user(identity)?;
    db.transaction(|tx| {
        let fresh = crate::auth::fresh_caller(tx, caller.id)?;
        let current = Some(crate::auth::fresh_identity(
            &fresh,
            crate::actor::Transport::Web,
        ));
        let minimum = if authz::authz_enforced_conn(tx)? {
            Role::Maintainer
        } else {
            Role::Lead
        };
        authz::require_role_conn(tx, &current, project_id, minimum)?;
        let owns = |id| {
            let actual = queries::get_resource_project_id(tx, queries::ResourceTable::Labels, id)?;
            if actual == project_id {
                Ok(())
            } else {
                Err(LificError::BadRequest(
                    "cannot change labels across projects".into(),
                ))
            }
        };
        match command {
            LabelCommand::Create { name, color } => {
                queries::create_label(
                    tx,
                    &crate::db::models::CreateLabel {
                        project_id,
                        name,
                        color,
                    },
                )?;
            }
            LabelCommand::Rename { id, name } => {
                owns(id)?;
                queries::update_label(
                    tx,
                    id,
                    &crate::db::models::UpdateLabel {
                        name: Some(name),
                        ..Default::default()
                    },
                )?;
            }
            LabelCommand::Recolor { id, color } => {
                owns(id)?;
                queries::update_label(
                    tx,
                    id,
                    &crate::db::models::UpdateLabel {
                        color: Some(color),
                        ..Default::default()
                    },
                )?;
            }
            LabelCommand::Delete { id } => {
                owns(id)?;
                queries::delete_label(tx, id)?;
            }
            LabelCommand::Merge { id, into } => {
                owns(id)?;
                owns(into)?;
                queries::merge_label(tx, id, into)?;
            }
        }
        Ok(())
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectUpdated { project_id });
    Ok(())
}

#[cfg(test)]
#[path = "project_overview_write_tests.rs"]
mod write_tests;
