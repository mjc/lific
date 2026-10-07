use crate::{
    authz,
    db::{
        models::{ProjectRelation, Role},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

#[cfg(test)]
use crate::db::{
    DbPool,
    models::{Issue, ListIssuesQuery, Project},
};

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct DependencyGraphData {
    pub(crate) project: Project,
    pub(crate) issues: Vec<Issue>,
    pub(crate) relations: Vec<ProjectRelation>,
}

pub(crate) fn project_relations_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Vec<ProjectRelation>, LificError> {
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &identity, project_id, Role::Viewer)?;
    queries::list_project_relations(conn, project_id)
}

#[cfg(test)]
pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_identifier: &str,
) -> Result<DependencyGraphData, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    let user = crate::api::require_user(&current)?;
    let visible = crate::services::projects::sidebar_visibility(&tx, user.id)?;
    let projects = authz::filter_visible(
        queries::list_projects_for_user(&tx, user.id)?,
        &visible,
        |project| Some(project.id),
    );
    let project = projects
        .into_iter()
        .find(|project| project.identifier == project_identifier)
        .ok_or_else(|| LificError::NotFound(format!("Project {project_identifier} not found")))?;
    authz::require_role_conn(&tx, &current, project.id, Role::Viewer)?;

    let mut issues = Vec::new();
    let mut offset = 0_i64;
    loop {
        let mut page = queries::list_issues(
            &tx,
            &ListIssuesQuery {
                project_id: Some(project.id),
                limit: Some(queries::MAX_PAGE_LIMIT),
                offset: Some(offset),
                ..Default::default()
            },
        )?;
        let page_len = page.len();
        crate::services::issues::retain_visible_relations_conn(&tx, &current, &mut page)?;
        issues.extend(page);
        if page_len < queries::MAX_PAGE_LIMIT as usize {
            break;
        }
        offset += queries::MAX_PAGE_LIMIT;
    }
    let relations = project_relations_conn(&tx, &current, project.id)?;
    tx.commit()?;
    Ok(DependencyGraphData {
        project,
        issues,
        relations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Transport;
    use crate::db::models::{CreateIssue, Status};
    use crate::db::queries;

    #[test]
    fn load_returns_project_issues_and_bulk_relations_for_a_viewer() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let (first, second) = {
            let conn = db.write().unwrap();
            let first = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: "Graph source".into(),
                    status: Status::Active,
                    ..Default::default()
                },
            )
            .unwrap();
            let second = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: "Graph target".into(),
                    status: Status::Todo,
                    ..Default::default()
                },
            )
            .unwrap();
            queries::link_issues(&conn, first.id, second.id, "blocks").unwrap();
            (first, second)
        };
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));

        let data = load(&db, &identity, "MEM").unwrap();

        assert_eq!(data.project.id, project_id);
        assert_eq!(data.issues.len(), 2);
        assert!(data.issues.iter().any(|issue| issue.id == first.id));
        assert!(data.issues.iter().any(|issue| issue.id == second.id));
        assert_eq!(data.relations.len(), 1);
        assert_eq!(data.relations[0].source_id, first.id);
        assert_eq!(data.relations[0].target_id, second.id);
        assert_eq!(data.relations[0].relation_type, "blocks");
    }

    #[test]
    fn load_pages_all_issues_beyond_the_database_page_limit() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        {
            let conn = db.write().unwrap();
            for index in 0..503 {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: format!("Graph issue {index}"),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            conn.execute(
                "UPDATE issues SET sort_order=-1 WHERE project_id=?1 AND sequence=503",
                [project_id],
            )
            .unwrap();
        }
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));

        let data = load(&db, &identity, "MEM").unwrap();

        assert_eq!(data.issues.len(), 503);
        assert_eq!(data.issues.first().unwrap().sequence, 503);
        assert_eq!(data.issues.last().unwrap().sequence, 502);
    }

    #[test]
    fn graph_reads_recheck_revoked_role_on_the_snapshot() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project_id, viewer.id],
            )
            .unwrap();

        assert!(matches!(
            load(&db, &identity, "MEM"),
            Err(LificError::NotFound(_)) | Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn relation_read_helper_rechecks_revoked_membership() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project_id, viewer.id],
            )
            .unwrap();
        let conn = db.read().unwrap();

        assert!(matches!(
            project_relations_conn(&conn, &identity, project_id),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn relation_read_helper_rechecks_admin_demotion() {
        let (db, admin, _, _, _, _, project_id) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&admin, Transport::Web));
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
            .unwrap();
        let conn = db.read().unwrap();

        assert!(matches!(
            project_relations_conn(&conn, &identity, project_id),
            Err(LificError::Forbidden(_))
        ));
    }
}
