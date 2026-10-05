use super::*;
use crate::{
    actor::Transport,
    api::test_helpers::setup_membership_test,
    db::{models::CreateProject, queries},
};
use tokio::sync::broadcast::error::TryRecvError;

fn create(db: &DbPool, user: i64, name: &str) -> i64 {
    queries::project_groups::create_group(
        &db.write().unwrap(),
        user,
        &CreateProjectGroup { name: name.into() },
    )
    .unwrap()
    .id
}
fn group_ids(groups: &[ProjectGroup]) -> Vec<i64> {
    groups.iter().map(|group| group.id).collect()
}

#[tokio::test]
async fn catalog_joins_personal_order_and_only_owned_visible_group_members() {
    let (db, _, lead, _, viewer, _, project) = setup_membership_test();
    let own = create(&db, viewer.id, "Owned");
    create(&db, lead.id, "Other account group");
    let hidden = {
        let conn = db.write().unwrap();
        let hidden = queries::create_project(
            &conn,
            &CreateProject {
                name: "Hidden".into(),
                identifier: "HID".into(),
                lead_user_id: Some(lead.id),
                ..Default::default()
            },
        )
        .unwrap();
        for id in [hidden.id, project] {
            queries::project_groups::assign_project(&conn, viewer.id, id, Some(own)).unwrap();
        }
        hidden.id
    };
    let rows = load(
        &db,
        &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
    )
    .unwrap();
    assert_eq!(rows.user.id, viewer.id);
    assert_eq!(
        rows.projects
            .iter()
            .map(|p| (p.id, p.sort_order))
            .collect::<Vec<_>>(),
        [(project, 0)]
    );
    assert_eq!(group_ids(&rows.groups), [own]);
    assert_eq!(rows.groups[0].project_ids, [project]);
    assert!(!rows.projects.iter().any(|p| p.id == hidden));
}

#[tokio::test]
async fn catalog_rechecks_demoted_and_disabled_snapshot_identity() {
    let (db, admin, _, _, _, _, project) = setup_membership_test();
    let group = create(&db, admin.id, "Private");
    queries::project_groups::assign_project(&db.write().unwrap(), admin.id, project, Some(group))
        .unwrap();
    let caller = Some(crate::auth::fresh_identity(&admin, Transport::Web));
    assert_eq!(load(&db, &caller).unwrap().groups[0].project_ids, [project]);
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    let fresh = load(&db, &caller).unwrap();
    assert!(!fresh.user.is_admin);
    assert!(fresh.projects.is_empty());
    assert!(fresh.groups[0].project_ids.is_empty());
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [admin.id])
        .unwrap();
    assert!(matches!(load(&db, &caller), Err(LificError::Forbidden(_))));
}

#[tokio::test]
async fn create_rename_and_delete_commit_one_owned_event_each_without_deleting_projects() {
    let (db, _, _, _, viewer, _, project) = setup_membership_test();
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let group = create_group(
        &db,
        &hub,
        &caller,
        CreateProjectGroup {
            name: " Work ".into(),
        },
    )
    .unwrap();
    assert_eq!(group.name, "Work");
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    ));
    queries::project_groups::assign_project(
        &db.write().unwrap(),
        viewer.id,
        project,
        Some(group.id),
    )
    .unwrap();
    let renamed = rename_group(
        &db,
        &hub,
        &caller,
        group.id,
        UpdateProjectGroup {
            name: Some("Renamed".into()),
        },
    )
    .unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.project_ids, [project]);
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    ));
    assert!(delete_group(&db, &hub, &caller, group.id).unwrap());
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    ));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let rows = load(&db, &caller).unwrap();
    assert!(rows.groups.is_empty());
    assert_eq!(rows.projects[0].id, project);
}

#[tokio::test]
async fn revoked_caller_cannot_create_rename_delete_or_reorder_owned_groups() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let group = create(&db, viewer.id, "Keep");
    let caller = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    assert!(matches!(
        create_group(
            &db,
            &hub,
            &caller,
            CreateProjectGroup { name: "New".into() }
        ),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        rename_group(
            &db,
            &hub,
            &caller,
            group,
            UpdateProjectGroup {
                name: Some("Changed".into())
            }
        ),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        delete_group(&db, &hub, &caller, group),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        reorder_groups(&db, &hub, &caller, &[group]),
        Err(LificError::Forbidden(_))
    ));
    assert_eq!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap()[0].name,
        "Keep"
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn rename_filters_hidden_members_after_admin_demotion() {
    let (db, admin, _, _, _, _, project) = setup_membership_test();
    let group = create(&db, admin.id, "Old");
    queries::project_groups::assign_project(&db.write().unwrap(), admin.id, project, Some(group))
        .unwrap();
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    let rows = rename_group(
        &db,
        &RealtimeHub::new(),
        &Some(crate::auth::fresh_identity(&admin, Transport::Web)),
        group,
        UpdateProjectGroup {
            name: Some("New".into()),
        },
    )
    .unwrap();
    assert!(rows.project_ids.is_empty());
}

#[tokio::test]
async fn group_reorder_preserves_omitted_order_and_rejects_foreign_or_duplicate_ids_atomically() {
    let (db, _, lead, _, viewer, _, _) = setup_membership_test();
    let a = create(&db, viewer.id, "A");
    let b = create(&db, viewer.id, "B");
    let c = create(&db, viewer.id, "C");
    let foreign = create(&db, lead.id, "Foreign");
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    assert_eq!(
        group_ids(&reorder_groups(&db, &hub, &caller, &[c]).unwrap()),
        [c, a, b]
    );
    events.try_recv().unwrap();
    for bad in [vec![b, b], vec![b, foreign]] {
        assert!(reorder_groups(&db, &hub, &caller, &bad).is_err());
    }
    assert_eq!(
        group_ids(&queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap()),
        [c, a, b]
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn failed_later_group_rank_rolls_back_all_ranks_and_emits_nothing() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let a = create(&db, viewer.id, "A");
    let b = create(&db, viewer.id, "B");
    db.write().unwrap().execute_batch(&format!("CREATE TRIGGER fail_second BEFORE UPDATE OF sort_order ON project_groups WHEN NEW.id={a} BEGIN SELECT RAISE(ABORT,'reject later rank'); END;")).unwrap();
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    assert!(
        reorder_groups(
            &db,
            &hub,
            &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
            &[b, a]
        )
        .is_err()
    );
    assert_eq!(
        group_ids(&queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap()),
        [a, b]
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn project_order_is_personal_normalized_and_preserves_group_membership() {
    let (db, _, lead, _, viewer, _, project) = setup_membership_test();
    let group = create(&db, viewer.id, "Work");
    let other = {
        let conn = db.write().unwrap();
        let p = queries::create_project(
            &conn,
            &CreateProject {
                name: "Other".into(),
                identifier: "OTH".into(),
                lead_user_id: Some(viewer.id),
                ..Default::default()
            },
        )
        .unwrap();
        queries::project_groups::assign_project(&conn, viewer.id, project, Some(group)).unwrap();
        p.id
    };
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let rows = reorder_projects(
        &db,
        &hub,
        &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
        &[other],
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .map(|p| (p.id, p.sort_order))
            .collect::<Vec<_>>(),
        [(other, 0), (project, 1)]
    );
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectsReordered
    ));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(
        load(
            &db,
            &Some(crate::auth::fresh_identity(&viewer, Transport::Web))
        )
        .unwrap()
        .groups[0]
            .project_ids,
        [project]
    );
    let conn = db.read().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM user_project_order WHERE user_id=?1",
            [lead.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn invisible_project_order_is_rejected_without_preferences_or_events() {
    let (db, _, lead, _, viewer, _, project) = setup_membership_test();
    let hidden = queries::create_project(
        &db.write().unwrap(),
        &CreateProject {
            name: "Hidden".into(),
            identifier: "HID".into(),
            lead_user_id: Some(lead.id),
            ..Default::default()
        },
    )
    .unwrap()
    .id;
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    for bad in [vec![project, project], vec![hidden, project]] {
        assert!(
            reorder_projects(
                &db,
                &hub,
                &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
                &bad
            )
            .is_err()
        );
    }
    let count: i64 = db
        .read()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM user_project_order WHERE user_id=?1",
            [viewer.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn successful_projects_survive_failed_membership_read_and_groups_recover() {
    let (db, _, _, _, viewer, _, project) = setup_membership_test();
    let id = create(&db, viewer.id, "Work");
    queries::project_groups::assign_project(&db.write().unwrap(), viewer.id, project, Some(id))
        .unwrap();
    db.write()
        .unwrap()
        .execute_batch("ALTER TABLE project_group_items RENAME TO unavailable_group_items;")
        .unwrap();
    let partial = load(
        &db,
        &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
    )
    .unwrap();
    assert_eq!(
        partial.projects.iter().map(|p| p.id).collect::<Vec<_>>(),
        [project]
    );
    assert!(partial.groups.is_empty());
    assert!(!partial.groups_ready);
    assert!(!partial.group_error.is_empty());
    db.write()
        .unwrap()
        .execute_batch("ALTER TABLE unavailable_group_items RENAME TO project_group_items;")
        .unwrap();
    let recovered = load(
        &db,
        &Some(crate::auth::fresh_identity(&viewer, Transport::Web)),
    )
    .unwrap();
    assert!(recovered.groups_ready);
    assert!(recovered.group_error.is_empty());
    assert_eq!(recovered.groups[0].project_ids, [project]);
}
#[tokio::test]
async fn fresh_auth_failure_remains_fatal_during_group_storage_failure() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let caller = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    db.write()
        .unwrap()
        .execute_batch("ALTER TABLE project_group_items RENAME TO unavailable_group_items;")
        .unwrap();
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    assert!(matches!(load(&db, &caller), Err(LificError::Forbidden(_))));
}
