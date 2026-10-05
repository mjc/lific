use super::*;
use crate::db::{models::User, queries};
fn setup() -> (
    DbPool,
    User,
    User,
    i64,
    RealtimeHub,
    Option<ResolvedIdentity>,
    String,
) {
    let (db, _, lead, _, _, other, project) = crate::api::test_helpers::setup_membership_test();
    let token = queries::users::create_session(&db.write().unwrap(), lead.id, None)
        .unwrap()
        .token;
    let identity = Some(crate::auth::fresh_identity(&lead, Transport::Web));
    (
        db,
        lead,
        other,
        project,
        RealtimeHub::new(),
        identity,
        token,
    )
}
#[test]
fn overview_member_grants_require_recent_session_while_reductions_do_not() {
    let (db, _, other, project, hub, identity, token) = setup();
    let mut events = hub.subscribe();
    assert!(
        matches!(add(&db,&hub,&identity,None,project,other.id,"viewer"),Err(LificError::Forbidden(message)) if message=="recent authentication required")
    );
    assert!(events.try_recv().is_err());
    add(
        &db,
        &hub,
        &identity,
        Some(&token),
        project,
        other.id,
        "viewer",
    )
    .unwrap();
    events.try_recv().unwrap();
    assert!(events.try_recv().is_err());
    db.write()
        .unwrap()
        .execute(
            "UPDATE sessions SET created_at=datetime('now','-16 minutes')",
            [],
        )
        .unwrap();
    assert!(matches!(
        change_role(
            &db,
            &hub,
            &identity,
            Some(&token),
            project,
            other.id,
            "maintainer"
        ),
        Err(LificError::Forbidden(_))
    ));
    assert_eq!(
        members::get_member_role(&db.read().unwrap(), project, other.id).unwrap(),
        Some(Role::Viewer)
    );
    assert!(events.try_recv().is_err());
    remove(&db, &hub, &identity, project, other.id).unwrap();
    events.try_recv().unwrap();
    assert!(events.try_recv().is_err());
}
#[test]
fn overview_members_do_not_reuse_demoted_admin_or_revoked_lead_authority() {
    let (db, lead, other, project, hub, identity, token) = setup();
    members::remove_member(&db.write().unwrap(), project, lead.id).unwrap();
    assert!(matches!(
        add(
            &db,
            &hub,
            &identity,
            Some(&token),
            project,
            other.id,
            "viewer"
        ),
        Err(LificError::Forbidden(_))
    ));
    assert!(list(&db, &identity, project).is_err());
    let (db, admin, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
    let admin_identity = Some(crate::auth::fresh_identity(&admin, Transport::Web));
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    assert!(matches!(
        remove(&db, &hub, &admin_identity, project, viewer.id),
        Err(LificError::Forbidden(_))
    ));
    assert_eq!(
        members::get_member_role(&db.read().unwrap(), project, viewer.id).unwrap(),
        Some(Role::Viewer)
    );
}
#[test]
fn overview_members_keep_strict_add_role_validation_and_last_lead_guard() {
    let (db, lead, other, project, hub, identity, token) = setup();
    let mut events = hub.subscribe();
    assert!(matches!(
        change_role(&db, &hub, &identity, None, project, lead.id, "viewer"),
        Err(LificError::Conflict(_))
    ));
    assert!(matches!(
        remove(&db, &hub, &identity, project, lead.id),
        Err(LificError::Conflict(_))
    ));
    assert!(matches!(
        change_role(&db, &hub, &identity, None, project, lead.id, "unknown"),
        Err(LificError::BadRequest(_))
    ));
    assert!(events.try_recv().is_err());
    add(
        &db,
        &hub,
        &identity,
        Some(&token),
        project,
        other.id,
        "lead",
    )
    .unwrap();
    assert!(matches!(
        add(
            &db,
            &hub,
            &identity,
            Some(&token),
            project,
            other.id,
            "lead"
        ),
        Err(LificError::Conflict(_))
    ));
    change_role(&db, &hub, &identity, None, project, lead.id, "maintainer").unwrap();
    assert_eq!(
        members::get_member_role(&db.read().unwrap(), project, lead.id).unwrap(),
        Some(Role::Maintainer)
    );
    assert_eq!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectUpdated {
            project_id: project
        }
    );
    assert_eq!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectUpdated {
            project_id: project
        }
    );
    assert!(events.try_recv().is_err());
}
