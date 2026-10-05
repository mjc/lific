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
    pub(crate) web_auto_login: bool,
    pub(crate) counts: Result<IssueStatusCounts, LificError>,
    pub(crate) issues: Result<Vec<Issue>, LificError>,
    pub(crate) activity: Result<Vec<Activity>, LificError>,
    pub(crate) labels: Result<Vec<Label>, LificError>,
    pub(crate) pages: Result<Vec<Page>, LificError>,
    pub(crate) members: Result<Vec<MemberWithUser>, LificError>,
    pub(crate) leads: Result<Vec<super::project_form::LeadOption>, LificError>,
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
    // Existing shared policy owns roster and caller-personal group filtering.
    let leads = super::project_form::list_leads(db, identity);
    let groups = super::project_form::list_groups(db, identity);
    Ok(OverviewReads {
        user,
        projects,
        project,
        role,
        enforced: settings.authz_enforced,
        web_auto_login: settings.web_auto_login,
        counts,
        issues,
        activity,
        labels,
        pages,
        members,
        leads,
        groups,
    })
}

#[cfg(test)]
#[path = "project_overview_tests.rs"]
mod tests;
