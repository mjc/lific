//! Authorized audit history shared by native views, REST and MCP.

use rusqlite::Connection;

use crate::{
    authz,
    db::{
        DbPool,
        models::{Activity, ActivityFeed, ActorStat, Role},
        queries::{self, activity::ActivityScope},
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) fn list_activity(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    scope: ActivityScope,
    since: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ActivityFeed, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let feed = list_activity_conn(&tx, identity, scope, since, limit, offset)?;
    tx.commit()?;
    Ok(feed)
}

/// The caller supplies an existing read transaction when resolving a scope or
/// composing a native model. Authorization and history share that snapshot.
pub(crate) fn list_activity_conn(
    conn: &Connection,
    identity: &Option<ResolvedIdentity>,
    scope: ActivityScope,
    since: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ActivityFeed, LificError> {
    let current = crate::auth::refresh_identity(conn, identity.as_ref())?;
    list_activity_current(conn, &current, scope, since, limit, offset)
}

/// The identity was refreshed inside the caller's current transaction.
fn list_activity_current(
    conn: &Connection,
    current: &Option<ResolvedIdentity>,
    scope: ActivityScope,
    since: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<ActivityFeed, LificError> {
    let project_id = match scope {
        ActivityScope::Issue(id) => Some(queries::get_issue(conn, id)?.project_id),
        ActivityScope::Page(id) => queries::get_page(conn, id)?.project_id,
        ActivityScope::Plan(id) => Some(queries::plans::get_plan(conn, id)?.project_id),
        ActivityScope::Project(id) => Some(id),
    };
    match project_id {
        Some(id) => authz::require_role_conn(conn, current, id, Role::Viewer)?,
        None => authz::require_workspace_admin_conn(conn, current)?,
    }
    let visible = authz::visible_project_ids_conn(conn, current)?;
    queries::activity::list_activity_since_visible(
        conn,
        scope,
        since,
        limit,
        offset,
        visible.as_ref(),
    )
}

pub(crate) fn project_actors(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project: i64,
) -> Result<Vec<ActorStat>, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    authz::require_role_conn(&tx, &current, project, Role::Viewer)?;
    let actors = actor_stats_conn(&tx, project)?;
    tx.commit()?;
    Ok(actors)
}

/// Only called after project authorization in the same read transaction.
/// Match the existing actor endpoint: the rollup includes all project audits,
/// while feed rows independently redact references to inaccessible projects.
fn actor_stats_conn(conn: &Connection, project: i64) -> Result<Vec<ActorStat>, LificError> {
    queries::activity::actor_stats(conn, project)
}

/// Retained authorized rows, an optional fresh page, and all-time actors share one read snapshot.
pub(crate) struct RetainedProjectActivity {
    pub(crate) items: Vec<Activity>,
    pub(crate) page: Option<ActivityFeed>,
    pub(crate) actors: Vec<ActorStat>,
}

/// Select an explicit offset or append after the retained authorized history.
pub(crate) enum ProjectActivityPage {
    Offset { limit: i64, offset: i64 },
    Append { limit: i64 },
}

pub(crate) fn project_retained_snapshot(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project: i64,
    ids: &[i64],
    page: Option<ProjectActivityPage>,
) -> Result<RetainedProjectActivity, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    authz::require_role_conn(&tx, &current, project, Role::Viewer)?;
    let visible = authz::visible_project_ids_conn(&tx, &current)?;
    let items =
        queries::activity::list_project_activity_ids_visible(&tx, project, ids, visible.as_ref())?;
    let page = page
        .map(|page| {
            let (limit, offset) = match page {
                ProjectActivityPage::Offset { limit, offset } => (limit, offset),
                ProjectActivityPage::Append { limit } => (limit, items.len() as i64),
            };
            queries::activity::list_activity_since_visible(
                &tx,
                ActivityScope::Project(project),
                None,
                Some(limit),
                Some(offset),
                visible.as_ref(),
            )
        })
        .transpose()?;
    let actors = actor_stats_conn(&tx, project)?;
    tx.commit()?;
    Ok(RetainedProjectActivity {
        items,
        page,
        actors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        db::models::{CreateIssue, CreatePlan, CreateProject, Status, UpdateIssue, UpdatePlan},
    };

    struct Fixture {
        db: DbPool,
        identity: Option<ResolvedIdentity>,
        source: i64,
        allowed: i64,
        hidden: i64,
        project: i64,
        hidden_project: i64,
    }

    fn fixture() -> Fixture {
        let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
        let (source, allowed, hidden, hidden_project) = {
            let conn = db.write().unwrap();
            conn.execute(
                "UPDATE projects SET identifier='ACC' WHERE id=?1",
                [project],
            )
            .unwrap();
            let hidden_project = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Private hidden project".into(),
                    ..Default::default()
                },
            )
            .unwrap()
            .id;
            let issue = |project_id, title: &str| {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: title.into(),
                        status: Status::Active,
                        ..Default::default()
                    },
                )
                .unwrap()
                .id
            };
            (
                issue(project, "Visible source"),
                issue(project, "Visible target"),
                issue(hidden_project, "Private hidden issue"),
                hidden_project,
            )
        };
        Fixture {
            db,
            identity: Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
            source,
            allowed,
            hidden,
            project,
            hidden_project,
        }
    }

    fn feed(fixture: &Fixture, scope: ActivityScope) -> ActivityFeed {
        list_activity(&fixture.db, &fixture.identity, scope, None, Some(200), None).unwrap()
    }

    fn assert_private_references_absent(feed: &ActivityFeed) {
        let json = serde_json::to_string(feed).unwrap();
        assert!(
            !json.contains("HIDE-1"),
            "hidden reference reached authorized feed: {json}"
        );
        assert!(
            !json.contains("Private plan"),
            "hidden plan reached authorized feed: {json}"
        );
    }

    #[test]
    fn retained_project_snapshot_reloads_requested_ids_and_pages_from_same_authority() {
        let fixture = fixture();
        let expected = feed(&fixture, ActivityScope::Project(fixture.project));
        let foreign = queries::activity::list_activity(
            &fixture.db.read().unwrap(),
            ActivityScope::Project(fixture.hidden_project),
            None,
            None,
        )
        .unwrap()
        .items[0]
            .id;
        let ids = [
            expected.items[0].id,
            expected.items[1].id,
            expected.items[0].id,
            foreign,
        ];
        let snapshot = project_retained_snapshot(
            &fixture.db,
            &fixture.identity,
            fixture.project,
            &ids,
            Some(ProjectActivityPage::Offset {
                limit: 1,
                offset: 1,
            }),
        )
        .unwrap();
        assert_eq!(
            snapshot.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![expected.items[0].id, expected.items[1].id]
        );
        let page = snapshot.page.unwrap();
        assert_eq!(page.items[0].id, expected.items[1].id);
        assert_eq!(page.has_more, expected.items.len() > 2);
        assert!(!snapshot.actors.is_empty());
        let refreshed = project_retained_snapshot(
            &fixture.db,
            &fixture.identity,
            fixture.project,
            &ids,
            Some(ProjectActivityPage::Offset {
                limit: 1,
                offset: 0,
            }),
        )
        .unwrap();
        assert_eq!(refreshed.page.unwrap().items[0].id, expected.items[0].id);
        let retained =
            project_retained_snapshot(&fixture.db, &fixture.identity, fixture.project, &ids, None)
                .unwrap();
        assert!(retained.page.is_none());
        assert_eq!(retained.items.len(), 2);
    }

    #[test]
    fn retained_project_snapshot_reads_more_than_one_page_of_ids_without_truncation() {
        let fixture = fixture();
        let ids = {
            let conn = fixture.db.write().unwrap();
            (0..241).map(|index| {
                conn.execute("INSERT INTO audit_log(transport,entity_type,entity_id,project_id,issue_id,action,field,new_value) VALUES('system','issue',?1,?2,?1,'update','title',?3)",rusqlite::params![fixture.source,fixture.project,format!("Retained edit {index}")]).unwrap();
                conn.last_insert_rowid()
            }).collect::<Vec<_>>()
        };
        let snapshot =
            project_retained_snapshot(&fixture.db, &fixture.identity, fixture.project, &ids, None)
                .unwrap();
        assert_eq!(
            snapshot.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            ids.into_iter().rev().collect::<Vec<_>>()
        );
        assert!(snapshot.page.is_none());
    }

    #[test]
    fn retained_append_does_not_skip_rows_after_target_visibility_is_revoked() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        {
            let conn = fixture.db.write().unwrap();
            queries::members::upsert_member(&conn, fixture.hidden_project, user, Role::Viewer)
                .unwrap();
            for index in 0..12 {
                queries::update_issue(
                    &conn,
                    fixture.source,
                    &UpdateIssue {
                        title: Some(format!("Append visibility edit {index}")),
                        ..Default::default()
                    },
                )
                .unwrap();
                queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
                queries::unlink_issues(&conn, fixture.source, fixture.hidden).unwrap();
            }
        }
        let initial = list_activity(
            &fixture.db,
            &fixture.identity,
            ActivityScope::Project(fixture.project),
            None,
            Some(12),
            None,
        )
        .unwrap();
        let retained = initial.items.iter().map(|row| row.id).collect::<Vec<_>>();
        assert!(
            initial
                .items
                .iter()
                .any(|row| row.new_value.as_deref() == Some("HIDE-1"))
        );
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.hidden_project, user)
            .unwrap();
        let expected = feed(&fixture, ActivityScope::Project(fixture.project));
        let snapshot = project_retained_snapshot(
            &fixture.db,
            &fixture.identity,
            fixture.project,
            &retained,
            Some(ProjectActivityPage::Append { limit: 12 }),
        )
        .unwrap();
        assert!(snapshot.items.len() < retained.len());
        let expected_page = expected
            .items
            .iter()
            .skip(snapshot.items.len())
            .take(12)
            .map(|row| row.id)
            .collect::<Vec<_>>();
        let page = snapshot.page.unwrap();
        assert_eq!(
            page.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            expected_page,
            "Append offset counts fresh readable retained rows"
        );
        assert_private_references_absent(&page);
    }

    #[test]
    fn retained_project_snapshot_rechecks_target_visibility_and_membership() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        {
            let conn = fixture.db.write().unwrap();
            queries::members::upsert_member(&conn, fixture.hidden_project, user, Role::Viewer)
                .unwrap();
            queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
        }
        let feed = feed(&fixture, ActivityScope::Project(fixture.project));
        let ids = feed.items.iter().map(|row| row.id).collect::<Vec<_>>();
        assert!(
            feed.items
                .iter()
                .any(|row| row.new_value.as_deref() == Some("HIDE-1"))
        );
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.hidden_project, user)
            .unwrap();
        let snapshot =
            project_retained_snapshot(&fixture.db, &fixture.identity, fixture.project, &ids, None)
                .unwrap();
        assert!(
            !snapshot
                .items
                .iter()
                .any(|row| row.new_value.as_deref() == Some("HIDE-1"))
        );
        assert!(!snapshot.items.is_empty());
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.project, user)
            .unwrap();
        assert!(matches!(
            project_retained_snapshot(&fixture.db, &fixture.identity, fixture.project, &ids, None),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn project_snapshot_pages_visible_history_and_keeps_full_actor_rollup() {
        let fixture = fixture();
        {
            let conn = fixture.db.write().unwrap();
            for index in 0..4 {
                queries::update_issue(
                    &conn,
                    fixture.source,
                    &UpdateIssue {
                        title: Some(format!("Snapshot edit {index}")),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
        }
        let expected = feed(&fixture, ActivityScope::Project(fixture.project));
        let snapshot = project_retained_snapshot(
            &fixture.db,
            &fixture.identity,
            fixture.project,
            &[],
            Some(ProjectActivityPage::Offset {
                limit: 2,
                offset: 1,
            }),
        )
        .unwrap();
        let page = snapshot.page.unwrap();
        assert_eq!(
            page.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            expected
                .items
                .iter()
                .skip(1)
                .take(2)
                .map(|row| row.id)
                .collect::<Vec<_>>()
        );
        assert!(page.has_more);
        assert_private_references_absent(&page);
        let actors = project_actors(&fixture.db, &fixture.identity, fixture.project).unwrap();
        let raw =
            queries::activity::actor_stats(&fixture.db.read().unwrap(), fixture.project).unwrap();
        assert!(!raw.is_empty());
        assert_eq!(
            serde_json::to_value(&snapshot.actors).unwrap(),
            serde_json::to_value(&raw).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&actors).unwrap(),
            serde_json::to_value(&raw).unwrap()
        );
    }

    #[test]
    fn project_actor_snapshot_rechecks_revoked_membership() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.project, user)
            .unwrap();
        assert!(matches!(
            project_actors(&fixture.db, &fixture.identity, fixture.project),
            Err(LificError::Forbidden(_))
        ));
        assert!(matches!(
            project_retained_snapshot(
                &fixture.db,
                &fixture.identity,
                fixture.project,
                &[],
                Some(ProjectActivityPage::Offset {
                    limit: 50,
                    offset: 0
                })
            ),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn project_actor_snapshot_rechecks_disabled_account_and_anonymous_access() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        fixture
            .db
            .write()
            .unwrap()
            .execute("UPDATE users SET is_active=0 WHERE id=?1", [user])
            .unwrap();
        assert!(matches!(
            project_actors(&fixture.db, &fixture.identity, fixture.project),
            Err(LificError::Forbidden(_))
        ));
        assert!(matches!(
            project_retained_snapshot(
                &fixture.db,
                &fixture.identity,
                fixture.project,
                &[],
                Some(ProjectActivityPage::Offset {
                    limit: 50,
                    offset: 0
                })
            ),
            Err(LificError::Forbidden(_))
        ));
        assert!(project_actors(&fixture.db, &None, fixture.project).is_err());
        assert!(
            project_retained_snapshot(
                &fixture.db,
                &None,
                fixture.project,
                &[],
                Some(ProjectActivityPage::Offset {
                    limit: 50,
                    offset: 0
                })
            )
            .is_err()
        );
    }

    #[test]
    fn visibility_precedes_offset_overfetch_and_since_ordering() {
        let fixture = fixture();
        let expected = {
            let conn = fixture.db.write().unwrap();
            for index in 0..3 {
                queries::update_issue(
                    &conn,
                    fixture.source,
                    &UpdateIssue {
                        title: Some(format!("Allowed edit {index}")),
                        ..Default::default()
                    },
                )
                .unwrap();
                queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
                queries::unlink_issues(&conn, fixture.source, fixture.hidden).unwrap();
            }
            let mut items = queries::activity::list_activity(
                &conn,
                ActivityScope::Issue(fixture.source),
                Some(200),
                None,
            )
            .unwrap()
            .items;
            items.retain(|item| matches!(item.action.as_str(), "create" | "update"));
            items.into_iter().map(|item| item.id).collect::<Vec<_>>()
        };
        for (since, expected) in [
            (None, expected.clone()),
            (
                Some("1900-01-01 00:00:00"),
                expected.into_iter().rev().collect(),
            ),
        ] {
            for offset in [0, 2, 4] {
                let page = list_activity(
                    &fixture.db,
                    &fixture.identity,
                    ActivityScope::Issue(fixture.source),
                    since,
                    Some(2),
                    Some(offset),
                )
                .unwrap();
                assert_private_references_absent(&page);
                assert_eq!(
                    page.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                    expected
                        .iter()
                        .skip(usize::try_from(offset).unwrap())
                        .take(2)
                        .copied()
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    page.has_more,
                    expected.len() > usize::try_from(offset).unwrap() + 2
                );
            }
        }
    }

    #[test]
    fn issue_feed_excludes_plan_history_owned_by_another_project() {
        let fixture = fixture();
        {
            let conn = fixture.db.write().unwrap();
            queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id: fixture.hidden_project,
                    title: "Private plan".into(),
                    issue_id: Some(fixture.source),
                    steps: Vec::new(),
                },
            )
            .unwrap();
        }
        let raw = queries::activity::list_activity(
            &fixture.db.read().unwrap(),
            ActivityScope::Issue(fixture.source),
            None,
            None,
        )
        .unwrap();
        assert!(
            raw.items
                .iter()
                .any(|item| item.new_value.as_deref() == Some("Private plan"))
        );
        assert_private_references_absent(&feed(&fixture, ActivityScope::Issue(fixture.source)));
    }

    #[test]
    fn step_and_anchor_old_and_new_references_respect_target_visibility() {
        let fixture = fixture();
        let plan = {
            let conn = fixture.db.write().unwrap();
            let plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id: fixture.project,
                    title: "Allowed plan".into(),
                    issue_id: None,
                    steps: Vec::new(),
                },
            )
            .unwrap();
            let step =
                queries::plans::add_step(&conn, plan.id, None, "Allowed step", "", None).unwrap();
            for target in [
                Some(fixture.allowed),
                Some(fixture.hidden),
                Some(fixture.allowed),
                None,
            ] {
                queries::plans::update_plan(
                    &conn,
                    plan.id,
                    &UpdatePlan {
                        issue_id: Some(target),
                        ..Default::default()
                    },
                )
                .unwrap();
                queries::plans::set_step_issue(&conn, step, target).unwrap();
            }
            plan
        };
        for scope in [
            ActivityScope::Plan(plan.id),
            ActivityScope::Project(fixture.project),
        ] {
            let scoped = feed(&fixture, scope);
            assert_private_references_absent(&scoped);
            for field in ["anchor_issue", "issue"] {
                let changes = scoped
                    .items
                    .iter()
                    .filter(|item| item.field.as_deref() == Some(field))
                    .collect::<Vec<_>>();
                assert_eq!(
                    changes.len(),
                    2,
                    "only None->allowed and allowed->None survive for {field}"
                );
                assert!(
                    changes.iter().any(|item| item.old_value.is_none()
                        && item.new_value.as_deref() == Some("ACC-2"))
                );
                assert!(
                    changes
                        .iter()
                        .any(|item| item.old_value.as_deref() == Some("ACC-2")
                            && item.new_value.is_none())
                );
            }
        }
    }

    #[test]
    fn renamed_and_purged_targets_keep_history_and_reused_prefixes_do_not_reassign_it() {
        let fixture = fixture();
        {
            let conn = fixture.db.write().unwrap();
            queries::link_issues(&conn, fixture.source, fixture.allowed, "relates_to").unwrap();
            queries::unlink_issues(&conn, fixture.source, fixture.allowed).unwrap();
            queries::delete_issue(&conn, fixture.allowed).unwrap();
            conn.execute(
                "UPDATE issues SET deleted_at='2000-01-01 00:00:00' WHERE id=?1",
                [fixture.allowed],
            )
            .unwrap();
            assert_eq!(
                queries::trash::purge_tombstones(&conn, 1).unwrap().issues,
                1
            );
            conn.execute(
                "UPDATE projects SET identifier='REN' WHERE id=?1",
                [fixture.project],
            )
            .unwrap();
            queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "ACC".into(),
                    name: "Reused hidden prefix".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let scoped = feed(&fixture, ActivityScope::Issue(fixture.source));
        assert_eq!(
            scoped
                .items
                .iter()
                .filter(|item| item.old_value.as_deref() == Some("ACC-2")
                    || item.new_value.as_deref() == Some("ACC-2"))
                .count(),
            2
        );
    }

    #[test]
    fn imported_unknown_target_uses_project_scope_but_ambiguous_history_requires_all_owners() {
        let fixture = fixture();
        {
            let conn = fixture.db.write().unwrap();
            // Imported audit history may reference an issue with no surviving
            // lifecycle row. The known project still establishes its scope.
            for target in ["ACC-99", "HIDE-99"] {
                conn.execute("INSERT INTO audit_log(transport,entity_type,entity_id,entity_label,project_id,issue_id,action,field,new_value) VALUES('import','issue',?1,'ACC-1',?2,?1,'link','relates_to',?3)", rusqlite::params![fixture.source,fixture.project,target]).unwrap();
            }
            // A renamed project's old prefix may be reused. Conflicting
            // immutable snapshots must not make a hidden identifier readable.
            for (project, id) in [(fixture.project, 90001), (fixture.hidden_project, 90002)] {
                conn.execute("INSERT INTO audit_log(transport,entity_type,entity_id,entity_label,project_id,issue_id,action,new_value) VALUES('import','issue',?1,'ACC-88',?2,?1,'create','Imported target')", rusqlite::params![id,project]).unwrap();
            }
            conn.execute("INSERT INTO audit_log(transport,entity_type,entity_id,entity_label,project_id,issue_id,action,field,new_value) VALUES('import','issue',?1,'ACC-1',?2,?1,'link','relates_to','ACC-88')", rusqlite::params![fixture.source,fixture.project]).unwrap();
        }
        let scoped = feed(&fixture, ActivityScope::Issue(fixture.source));
        assert!(
            scoped
                .items
                .iter()
                .any(|item| item.new_value.as_deref() == Some("ACC-99"))
        );
        assert!(
            !scoped
                .items
                .iter()
                .any(|item| matches!(item.new_value.as_deref(), Some("HIDE-99" | "ACC-88")))
        );
    }

    #[test]
    fn authority_is_fresh_and_unrestricted_modes_preserve_raw_history() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        {
            let conn = fixture.db.write().unwrap();
            queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
            conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user])
                .unwrap();
        }
        let admin = {
            let user = queries::users::get_user_by_id(&fixture.db.read().unwrap(), user).unwrap();
            Some(crate::auth::fresh_identity(&user, Transport::Web))
        };
        let raw = queries::activity::list_activity(
            &fixture.db.read().unwrap(),
            ActivityScope::Issue(fixture.source),
            Some(200),
            None,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(
                list_activity(
                    &fixture.db,
                    &admin,
                    ActivityScope::Issue(fixture.source),
                    None,
                    Some(200),
                    None
                )
                .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&raw).unwrap()
        );
        fixture
            .db
            .write()
            .unwrap()
            .execute("UPDATE users SET is_admin=0 WHERE id=?1", [user])
            .unwrap();
        assert_private_references_absent(
            &list_activity(
                &fixture.db,
                &admin,
                ActivityScope::Issue(fixture.source),
                None,
                Some(200),
                None,
            )
            .unwrap(),
        );
        {
            let conn = fixture.db.write().unwrap();
            conn.execute("UPDATE instance_settings SET authz_enforced=0", [])
                .unwrap();
        }
        assert_eq!(
            serde_json::to_value(feed(&fixture, ActivityScope::Issue(fixture.source))).unwrap(),
            serde_json::to_value(&raw).unwrap()
        );
        fixture
            .db
            .write()
            .unwrap()
            .execute("UPDATE users SET is_active=0 WHERE id=?1", [user])
            .unwrap();
        assert!(matches!(
            list_activity(
                &fixture.db,
                &admin,
                ActivityScope::Issue(fixture.source),
                None,
                None,
                None
            ),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn revoked_membership_rechecks_both_scope_and_target_visibility() {
        let fixture = fixture();
        let user = fixture.identity.as_ref().unwrap().user.id;
        {
            let conn = fixture.db.write().unwrap();
            queries::members::upsert_member(&conn, fixture.hidden_project, user, Role::Viewer)
                .unwrap();
            queries::link_issues(&conn, fixture.source, fixture.hidden, "relates_to").unwrap();
        }
        assert!(
            feed(&fixture, ActivityScope::Issue(fixture.source))
                .items
                .iter()
                .any(|item| item.new_value.as_deref() == Some("HIDE-1"))
        );
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.hidden_project, user)
            .unwrap();
        assert_private_references_absent(&feed(&fixture, ActivityScope::Issue(fixture.source)));
        queries::members::remove_member(&fixture.db.write().unwrap(), fixture.project, user)
            .unwrap();
        assert!(matches!(
            list_activity(
                &fixture.db,
                &fixture.identity,
                ActivityScope::Issue(fixture.source),
                None,
                None,
                None
            ),
            Err(LificError::Forbidden(_))
        ));
    }
}
