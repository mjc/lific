use crate::{
    authz,
    db::{
        DbPool,
        models::{Issue, ListIssuesQuery, Project, ProjectRelation, Role},
        queries,
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug)]
pub(crate) struct DependencyGraphData {
    pub(crate) project: Project,
    pub(crate) issues: Vec<Issue>,
    pub(crate) relations: Vec<ProjectRelation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RelationMutation {
    pub(crate) source_project_id: i64,
    pub(crate) source_issue_id: i64,
    pub(crate) target_project_id: i64,
    pub(crate) target_issue_id: i64,
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

pub(crate) fn load_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<DependencyGraphData, LificError> {
    let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &current, project_id, Role::Viewer)?;
    let project = queries::get_project(conn, project_id)?;
    let mut issues = Vec::new();
    let mut offset = 0_i64;
    loop {
        let mut page = queries::list_issues(
            conn,
            &ListIssuesQuery {
                project_id: Some(project.id),
                limit: Some(queries::MAX_PAGE_LIMIT),
                offset: Some(offset),
                ..Default::default()
            },
        )?;
        let count = page.len();
        crate::services::issues::retain_visible_relations_conn(conn, &current, &mut page)?;
        issues.extend(page);
        if count < queries::MAX_PAGE_LIMIT as usize {
            break;
        }
        offset += queries::MAX_PAGE_LIMIT;
    }
    let relations = project_relations_conn(conn, &current, project.id)?;
    Ok(DependencyGraphData {
        project,
        issues,
        relations,
    })
}

pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_identifier: &str,
) -> Result<DependencyGraphData, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let project_id = queries::resolve_project_identifier(&tx, project_identifier)?;
    let graph = load_conn(&tx, identity, project_id)?;
    tx.commit()?;
    Ok(graph)
}

pub(crate) fn link(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    source_identifier: &str,
    target_identifier: &str,
    relation_type: &str,
    expected_project_id: Option<i64>,
) -> Result<RelationMutation, LificError> {
    let changed = db.transaction(|conn| {
        let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
        let (source, target) = resolve_pair(conn, source_identifier, target_identifier)?;
        ensure_graph_project(&source, &target, expected_project_id)?;
        require_maintainers(conn, &current, source.project_id, target.project_id)?;
        queries::link_issues(conn, source.id, target.id, relation_type)?;
        Ok(RelationMutation {
            source_project_id: source.project_id,
            source_issue_id: source.id,
            target_project_id: target.project_id,
            target_issue_id: target.id,
        })
    })?;
    send_mutation_events(realtime, changed, MutationEvent::Linked);
    Ok(changed)
}

pub(crate) fn unlink(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    source_identifier: &str,
    target_identifier: &str,
    expected_project_id: Option<i64>,
) -> Result<RelationMutation, LificError> {
    let changed = db.transaction(|conn| {
        let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
        let (source, target) = resolve_pair(conn, source_identifier, target_identifier)?;
        ensure_graph_project(&source, &target, expected_project_id)?;
        require_maintainers(conn, &current, source.project_id, target.project_id)?;
        queries::unlink_issues(conn, source.id, target.id)?;
        Ok(RelationMutation {
            source_project_id: source.project_id,
            source_issue_id: source.id,
            target_project_id: target.project_id,
            target_issue_id: target.id,
        })
    })?;
    send_mutation_events(realtime, changed, MutationEvent::Unlinked);
    Ok(changed)
}

pub(crate) fn reverse(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    source_identifier: &str,
    target_identifier: &str,
    expected_project_id: Option<i64>,
) -> Result<Vec<String>, LificError> {
    let (changed, relation_types) = db.transaction(|conn| {
        let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
        let (source, target) = resolve_pair(conn, source_identifier, target_identifier)?;
        ensure_graph_project(&source, &target, expected_project_id)?;
        require_maintainers(conn, &current, source.project_id, target.project_id)?;
        let relation_types = queries::reverse_relation(conn, source.id, target.id)?;
        Ok((
            RelationMutation {
                source_project_id: source.project_id,
                source_issue_id: source.id,
                target_project_id: target.project_id,
                target_issue_id: target.id,
            },
            relation_types,
        ))
    })?;
    send_mutation_events(realtime, changed, MutationEvent::Reversed);
    Ok(relation_types)
}

fn resolve_pair(
    conn: &rusqlite::Connection,
    source_identifier: &str,
    target_identifier: &str,
) -> Result<(Issue, Issue), LificError> {
    let source_id = queries::resolve_identifier(conn, source_identifier)?;
    let target_id = queries::resolve_identifier(conn, target_identifier)?;
    Ok((
        queries::get_issue(conn, source_id)?,
        queries::get_issue(conn, target_id)?,
    ))
}

fn ensure_graph_project(
    source: &Issue,
    target: &Issue,
    expected_project_id: Option<i64>,
) -> Result<(), LificError> {
    if let Some(project_id) = expected_project_id
        && (source.project_id != project_id || target.project_id != project_id)
    {
        return Err(LificError::NotFound(
            "issue not found in this project".into(),
        ));
    }
    Ok(())
}

fn require_maintainers(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    source_project_id: i64,
    target_project_id: i64,
) -> Result<(), LificError> {
    authz::require_role_conn(conn, identity, source_project_id, Role::Maintainer)?;
    authz::require_role_conn(conn, identity, target_project_id, Role::Maintainer)
}

#[derive(Clone, Copy)]
enum MutationEvent {
    Linked,
    Unlinked,
    Reversed,
}

fn send_mutation_events(realtime: &RealtimeHub, mutation: RelationMutation, event: MutationEvent) {
    let endpoints = [
        (mutation.source_project_id, mutation.source_issue_id),
        (mutation.target_project_id, mutation.target_issue_id),
    ];
    for (project_id, issue_id) in endpoints {
        if matches!(event, MutationEvent::Unlinked | MutationEvent::Reversed) {
            realtime.send(RealtimeEvent::IssueUnlinked {
                project_id,
                issue_id,
            });
        }
        if matches!(event, MutationEvent::Linked | MutationEvent::Reversed) {
            realtime.send(RealtimeEvent::IssueLinked {
                project_id,
                issue_id,
            });
        }
    }
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
