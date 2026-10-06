//! Actual HTTP adapter must consume the same fresh owned-group services.
use crate::api::test_helpers::*;
use crate::db::{models::CreateProjectGroup, queries};
use axum::http::StatusCode;
use tokio::sync::broadcast::error::TryRecvError;

fn group(db: &crate::db::DbPool, owner: i64) -> i64 {
    queries::project_groups::create_group(
        &db.write().unwrap(),
        owner,
        &CreateProjectGroup {
            name: "Keep".into(),
        },
    )
    .unwrap()
    .id
}
#[tokio::test]
async fn revoked_snapshot_cannot_create_owned_group_through_rest() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let app = app_as_user_with_realtime(db.clone(), &viewer, hub);
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    let response = json_post(
        &app,
        "/api/project-groups",
        serde_json::json!({"name":"New"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}
#[tokio::test]
async fn revoked_snapshot_cannot_rename_owned_group_through_rest() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let id = group(&db, viewer.id);
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let app = app_as_user_with_realtime(db.clone(), &viewer, hub);
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    let response = json_patch(
        &app,
        &format!("/api/project-groups/{id}"),
        serde_json::json!({"name":"Changed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id).unwrap()[0].name,
        "Keep"
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}
#[tokio::test]
async fn revoked_snapshot_cannot_delete_owned_group_through_rest() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let id = group(&db, viewer.id);
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let app = app_as_user_with_realtime(db.clone(), &viewer, hub);
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    let response = json_delete(&app, &format!("/api/project-groups/{id}")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        queries::project_groups::list_groups(&db.read().unwrap(), viewer.id)
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}
#[tokio::test]
async fn demoted_snapshot_rename_response_filters_hidden_project_members() {
    let (db, admin, _, _, _, _, project) = setup_membership_test();
    let id = group(&db, admin.id);
    queries::project_groups::assign_project(&db.write().unwrap(), admin.id, project, Some(id))
        .unwrap();
    let app = app_as_user(db.clone(), &admin);
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    let response = json_patch(
        &app,
        &format!("/api/project-groups/{id}"),
        serde_json::json!({"name":"New"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = parse_json(response).await;
    assert_eq!(body["name"], "New");
    assert_eq!(body["project_ids"], serde_json::json!([]));
}
