use std::collections::BTreeMap;

use rusqlite::params;
use tokio::sync::broadcast::error::TryRecvError;

use super::{assign_created_project, assign_project, form_catalog, list_groups, list_leads};
use crate::{
    actor::{ActorCtx, Transport},
    db::{
        DbPool,
        models::{CreateProject, CreateProjectGroup, CreateUser, Role, User},
        queries,
    },
    error::LificError,
    realtime::{EventVisibility, RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

struct Fixture {
    db: DbPool,
    admin: User,
    owner: User,
    other: User,
    inactive: User,
    bot: User,
    visible: i64,
    hidden: i64,
    first_group: i64,
    second_group: i64,
    foreign_group: i64,
}

fn fixture() -> Fixture {
    let hash = queries::users::hash_password("catalog-contract-password").unwrap();
    let db = crate::db::open_memory().unwrap();
    let (
        admin,
        owner,
        other,
        inactive,
        bot,
        visible,
        hidden,
        first_group,
        second_group,
        foreign_group,
    ) = {
        let conn = db.write().unwrap();
        queries::settings::update(
            &conn,
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let insert = |username: &str, is_admin| {
            let input = CreateUser {
                username: username.into(),
                email: format!("{username}@test.local"),
                password: "catalog-contract-password".into(),
                display_name: None,
                is_admin,
                is_bot: false,
            };
            queries::users::validate_new_user(&input).unwrap();
            queries::users::insert_user_with_hash(&conn, &input, &hash).unwrap()
        };
        let admin = insert("admin", true);
        let owner = insert("owner", false);
        let other = insert("other", false);
        let mut inactive = insert("inactive", false);
        conn.execute(
            "UPDATE users SET is_active = 0 WHERE id = ?1",
            [inactive.id],
        )
        .unwrap();
        inactive.is_active = false;
        let bot = queries::users::create_bot_user(&conn, owner.id, "agent", "Agent", None).unwrap();
        let create_project = |identifier: &str| {
            queries::create_project(
                &conn,
                &CreateProject {
                    name: identifier.into(),
                    identifier: identifier.into(),
                    lead_user_id: Some(other.id),
                    ..Default::default()
                },
            )
            .unwrap()
            .id
        };
        let visible = create_project("VIS");
        let hidden = create_project("HIDE");
        queries::members::upsert_member(&conn, visible, owner.id, Role::Viewer).unwrap();
        let group = |user_id, name: &str| {
            queries::project_groups::create_group(
                &conn,
                user_id,
                &CreateProjectGroup { name: name.into() },
            )
            .unwrap()
            .id
        };
        let first_group = group(owner.id, "First");
        let second_group = group(owner.id, "Empty");
        let foreign_group = group(other.id, "Foreign");
        for project in [visible, hidden] {
            queries::project_groups::assign_project(&conn, owner.id, project, Some(first_group))
                .unwrap();
        }
        queries::project_groups::assign_project(&conn, other.id, visible, Some(foreign_group))
            .unwrap();
        (
            admin,
            owner,
            other,
            inactive,
            bot,
            visible,
            hidden,
            first_group,
            second_group,
            foreign_group,
        )
    };
    Fixture {
        db,
        admin,
        owner,
        other,
        inactive,
        bot,
        visible,
        hidden,
        first_group,
        second_group,
        foreign_group,
    }
}

fn identity(user: &User) -> ResolvedIdentity {
    crate::auth::fresh_identity(user, Transport::Web)
}

fn group_items(db: &DbPool) -> Vec<(i64, i64)> {
    let conn = db.read().unwrap();
    conn.prepare(
        "SELECT group_id, project_id FROM project_group_items ORDER BY group_id, project_id",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn table_counts(db: &DbPool) -> BTreeMap<String, i64> {
    let conn = db.read().unwrap();
    let mut statement = conn.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|name| {
            let name = name.unwrap();
            let quoted = name.replace('"', "\"\"");
            let count = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
                    row.get(0)
                })
                .unwrap();
            (name, count)
        })
        .collect()
}

#[test]
fn authenticated_roster_retains_inactive_humans_database_order_and_exact_safe_fields() {
    let f = fixture();
    let caller = Some(identity(&f.owner));
    let leads = list_leads(&f.db, &caller).unwrap();
    let expected = queries::users::list_users(&f.db.read().unwrap())
        .unwrap()
        .into_iter()
        .filter(|user| !user.is_bot)
        .map(|user| user.id)
        .collect::<Vec<_>>();
    assert_eq!(
        leads.iter().map(|user| user.id).collect::<Vec<_>>(),
        expected
    );
    assert!(
        leads
            .iter()
            .any(|user| user.id == f.inactive.id && !user.is_active)
    );
    assert!(leads.iter().all(|user| user.id != f.bot.id));
    let serialized = serde_json::to_value(&leads).unwrap();
    for item in serialized.as_array().unwrap() {
        let fields = item.as_object().unwrap();
        assert_eq!(fields.len(), 6);
        for key in [
            "id",
            "username",
            "display_name",
            "is_admin",
            "is_active",
            "created_at",
        ] {
            assert!(fields.contains_key(key));
        }
    }
    assert!(
        matches!(list_leads(&f.db, &None), Err(LificError::Forbidden(message)) if message == "authentication required")
    );
}

#[test]
fn catalog_keeps_only_owned_groups_and_visible_project_ids_without_losing_empty_groups() {
    let f = fixture();
    let caller = Some(identity(&f.owner));
    let (leads, groups) = form_catalog(&f.db, &caller).unwrap();
    assert_eq!(
        groups.iter().map(|group| group.id).collect::<Vec<_>>(),
        vec![f.first_group, f.second_group]
    );
    assert_eq!(groups[0].project_ids, vec![f.visible]);
    assert!(groups[1].project_ids.is_empty());
    assert!(groups.iter().all(|group| group.user_id == f.owner.id));
    assert!(leads.iter().any(|user| user.id == f.inactive.id));
    assert_eq!(
        serde_json::to_value(&groups).unwrap(),
        serde_json::to_value(list_groups(&f.db, &caller).unwrap()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&leads).unwrap(),
        serde_json::to_value(list_leads(&f.db, &caller).unwrap()).unwrap()
    );
    // Listing filters its result; it must not delete stale stored membership.
    assert!(group_items(&f.db).contains(&(f.first_group, f.hidden)));
    queries::settings::update(
        &f.db.write().unwrap(),
        queries::settings::InstanceSettingsPatch {
            authz_enforced: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let legacy = list_groups(&f.db, &caller).unwrap();
    assert!(legacy[0].project_ids.contains(&f.visible));
    assert!(legacy[0].project_ids.contains(&f.hidden));
    assert!(
        matches!(form_catalog(&f.db, &None), Err(LificError::Forbidden(message)) if message == "authentication required")
    );
}

#[test]
fn catalog_refreshes_captured_admin_authority_and_refuses_deactivated_callers() {
    let f = fixture();
    let caller = Some(identity(&f.admin));
    let group = {
        let conn = f.db.write().unwrap();
        let group = queries::project_groups::create_group(
            &conn,
            f.admin.id,
            &CreateProjectGroup {
                name: "Admin".into(),
            },
        )
        .unwrap();
        queries::project_groups::assign_project(&conn, f.admin.id, f.hidden, Some(group.id))
            .unwrap();
        group.id
    };
    assert_eq!(
        list_groups(&f.db, &caller).unwrap()[0].project_ids,
        vec![f.hidden]
    );
    f.db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [f.admin.id])
        .unwrap();
    let (_, groups) = form_catalog(&f.db, &caller).unwrap();
    assert_eq!(groups[0].id, group);
    assert!(groups[0].project_ids.is_empty());
    f.db.write()
        .unwrap()
        .execute("UPDATE users SET is_active = 0 WHERE id = ?1", [f.admin.id])
        .unwrap();
    for result in [
        list_groups(&f.db, &caller),
        form_catalog(&f.db, &caller).map(|(_, groups)| groups),
    ] {
        assert!(
            matches!(result, Err(LificError::Forbidden(message)) if message == "authentication required")
        );
    }
}

#[test]
fn viewer_can_move_and_ungroup_only_own_membership_and_publish_to_owner_and_admin() {
    let f = fixture();
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(identity(&f.owner));
    assign_created_project(&f.db, &hub, &caller, f.visible, f.second_group).unwrap();
    let items = group_items(&f.db);
    assert!(!items.contains(&(f.first_group, f.visible)));
    assert!(items.contains(&(f.second_group, f.visible)));
    assert!(items.contains(&(f.foreign_group, f.visible)));
    let event = events.try_recv().unwrap();
    assert_eq!(event.event, RealtimeEvent::ProjectGroupsChanged);
    for (user, expected) in [
        (&f.owner, EventVisibility::Visible),
        (&f.admin, EventVisibility::Visible),
        (&f.other, EventVisibility::Hidden),
    ] {
        assert_eq!(
            crate::realtime::visible_to(&f.db, &crate::auth::fresh_auth_user(user), &event),
            expected
        );
    }
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    assign_project(&f.db, &hub, &caller, f.visible, None).unwrap();
    let items = group_items(&f.db);
    assert!(!items.contains(&(f.second_group, f.visible)));
    assert!(items.contains(&(f.foreign_group, f.visible)));
    assert_eq!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn assignment_refusals_keep_all_rows_and_publish_nothing_including_stale_admin_snapshot() {
    for kind in [
        "foreign-group",
        "missing-group",
        "hidden-project",
        "demoted-admin",
        "disabled-caller",
        "removed-viewer",
        "anonymous",
    ] {
        let f = fixture();
        let hub = RealtimeHub::new();
        let mut events = hub.subscribe();
        let mut caller = Some(identity(&f.owner));
        let mut project = f.visible;
        let mut group = f.second_group;
        {
            let conn = f.db.write().unwrap();
            match kind {
                "foreign-group" => group = f.foreign_group,
                "missing-group" => group = i64::MAX,
                "hidden-project" => {
                    project = f.hidden;
                    group = f.foreign_group;
                }
                "demoted-admin" => {
                    caller = Some(identity(&f.admin));
                    group = queries::project_groups::create_group(
                        &conn,
                        f.admin.id,
                        &CreateProjectGroup {
                            name: "Former admin".into(),
                        },
                    )
                    .unwrap()
                    .id;
                    conn.execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [f.admin.id])
                        .unwrap();
                }
                "disabled-caller" => {
                    conn.execute("UPDATE users SET is_active = 0 WHERE id = ?1", [f.owner.id])
                        .unwrap();
                }
                "removed-viewer" => {
                    queries::members::remove_member(&conn, f.visible, f.owner.id).unwrap();
                }
                "anonymous" => caller = None,
                _ => unreachable!(),
            }
        }
        let before = (group_items(&f.db), table_counts(&f.db));
        let error = assign_created_project(&f.db, &hub, &caller, project, group).unwrap_err();
        match kind {
            "foreign-group" | "missing-group" => assert!(matches!(error, LificError::NotFound(_))),
            "disabled-caller" => assert!(
                matches!(error,LificError::Forbidden(message) if message == "authentication required")
            ),
            _ => assert!(
                matches!(error,LificError::Forbidden(message) if message == "requires at least 'viewer' access to this project")
            ),
        }
        assert_eq!(group_items(&f.db), before.0);
        assert_eq!(table_counts(&f.db), before.1);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    }
}

#[tokio::test]
async fn failed_group_assignment_leaves_shared_creation_committed_once_with_original_event_and_audit()
 {
    let f = fixture();
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(identity(&f.owner));
    let project = crate::actor::scope(
        ActorCtx {
            user_id: Some(f.owner.id),
            transport: Transport::Web,
        },
        async {
            super::super::projects::create_project(
                &f.db,
                &hub,
                &caller,
                None,
                CreateProject {
                    name: "Created once".into(),
                    identifier: "ONCE".into(),
                    ..Default::default()
                },
            )
            .unwrap()
        },
    )
    .await;
    assert_eq!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectCreated {
            project_id: project.id
        }
    );
    let before = table_counts(&f.db);
    assert!(matches!(
        assign_created_project(&f.db, &hub, &caller, project.id, f.foreign_group),
        Err(LificError::NotFound(_))
    ));
    assert_eq!(table_counts(&f.db), before);
    let conn = f.db.read().unwrap();
    assert_eq!(
        queries::get_project(&conn, project.id).unwrap().identifier,
        "ONCE"
    );
    let audit_count:i64=conn.query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",params![project.id,f.owner.id],|row|row.get(0)).unwrap();
    assert_eq!(audit_count, 1);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}
