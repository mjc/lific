use super::super::{
    actions::Write,
    model::{Catalog, EditTarget, State},
};
use super::*;
use crate::{
    actor::Transport, api::test_helpers::setup_membership_test, db::queries,
    realtime::RealtimeEvent,
};
fn write(owner: i64, name: &str) -> Write {
    let mut state = State::new(Catalog {
        owner,
        generation: 0,
        projects: Vec::new(),
        groups: Vec::new(),
    });
    assert!(state.begin_edit(EditTarget::New { project: None }, "trigger".into()));
    state.draft(name.into());
    let (target, name) = state.begin_save().unwrap();
    Write::SaveGroup {
        token: state.save_token().unwrap(),
        target,
        name,
    }
}
#[tokio::test]
async fn canceled_send_has_zero_database_writes_and_retries_once_with_a_new_receipt() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let store = SidebarWriteStore::default();
    let old = store
        .reserve(viewer.id, write(viewer.id, "Before abort"))
        .unwrap();
    assert_eq!(old.len(), 48);
    assert!(
        old.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id)
            .unwrap()
            .is_empty()
    );
    assert!(events.try_recv().is_err());
    let stopped = store.recover(&db, &identity, &old).unwrap();
    assert!(stopped.error.is_some());
    assert_eq!(
        store.execute(&db, &hub, &identity, &old).unwrap(),
        stopped,
        "A late old request replays its cancellation, not a commit."
    );
    assert!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id)
            .unwrap()
            .is_empty()
    );
    assert!(events.try_recv().is_err());
    let retry = store
        .reserve(viewer.id, write(viewer.id, "Before abort"))
        .unwrap();
    assert_ne!(retry, old);
    assert!(
        store
            .execute(&db, &hub, &identity, &retry)
            .unwrap()
            .error
            .is_none()
    );
    let groups = queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Before abort");
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    ));
    assert!(events.try_recv().is_err());
}
#[tokio::test]
async fn lost_success_reply_replays_one_committed_group_and_exactly_one_event() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let hub = RealtimeHub::new();
    let mut events = hub.subscribe();
    let store = SidebarWriteStore::default();
    let key = store
        .reserve(viewer.id, write(viewer.id, "After commit"))
        .unwrap();
    let committed = store.execute(&db, &hub, &identity, &key).unwrap();
    assert!(committed.error.is_none());
    assert_eq!(store.recover(&db, &identity, &key).unwrap(), committed);
    assert_eq!(
        store.execute(&db, &hub, &identity, &key).unwrap(),
        committed
    );
    let groups = queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "After commit");
    assert!(matches!(
        events.try_recv().unwrap().event,
        RealtimeEvent::ProjectGroupsChanged
    ));
    assert!(events.try_recv().is_err());
}
#[tokio::test]
async fn foreign_account_cannot_execute_or_consume_an_owners_ready_receipt() {
    let (db, _, lead, _, viewer, _, _) = setup_membership_test();
    let owner = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let foreign = Some(crate::auth::fresh_identity(&lead, Transport::Web));
    let hub = RealtimeHub::new();
    let store = SidebarWriteStore::default();
    let key = store
        .reserve(viewer.id, write(viewer.id, "Owner only"))
        .unwrap();
    assert!(matches!(
        store.recover(&db, &foreign, &key),
        Err(LificError::NotFound(_))
    ));
    assert!(matches!(
        store.execute(&db, &hub, &foreign, &key),
        Err(LificError::NotFound(_))
    ));
    assert!(
        store
            .execute(&db, &hub, &owner, &key)
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id)
            .unwrap()
            .len(),
        1
    );
    assert!(
        queries::project_groups::list_groups(&db.read().unwrap(), lead.id)
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn disabled_caller_cannot_replay_even_a_committed_receipt() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let hub = RealtimeHub::new();
    let store = SidebarWriteStore::default();
    let key = store
        .reserve(viewer.id, write(viewer.id, "Before revocation"))
        .unwrap();
    assert!(
        store
            .execute(&db, &hub, &identity, &key)
            .unwrap()
            .error
            .is_none()
    );
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    assert!(matches!(
        store.execute(&db, &hub, &identity, &key),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        store.recover(&db, &identity, &key),
        Err(LificError::Forbidden(_))
    ));
}
