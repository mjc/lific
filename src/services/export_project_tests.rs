use super::*;
use crate::{
    actor::Transport,
    api::test_helpers::setup_membership_test,
    db::{
        models::{CreateIssue, CreatePage, CreateProject},
        queries,
    },
    services::export::{EXPORT_TEST_GATE, ExportTestGate},
};
use http_body_util::BodyExt;
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

/// Hold the real blocking worker after preflight and before its database read.
async fn held_export(
    db: &DbPool,
    identity: ResolvedIdentity,
    change: impl FnOnce(&DbPool),
) -> Result<Response, LificError> {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let gate = Arc::new(ExportTestGate::new(started_tx, release_rx));
    let work_db = db.clone();
    let task = tokio::spawn(EXPORT_TEST_GATE.scope(gate, async move {
        project(work_db, &Some(identity), "MEM".into(), Some("json".into())).await
    }));
    tokio::time::timeout(Duration::from_secs(10), started_rx)
        .await
        .expect("export worker must reach its real gate")
        .expect("export worker must announce admission");
    change(db);
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("released export worker must finish")
        .unwrap()
}

#[tokio::test]
async fn project_export_keeps_authorized_id_when_identifier_is_reassigned() {
    let (db, _, lead, _, viewer, _, allowed) = setup_membership_test();
    let hidden = {
        let conn = db.write().unwrap();
        let hidden = queries::create_project(
            &conn,
            &CreateProject {
                name: "Private project".into(),
                identifier: "HID".into(),
                lead_user_id: Some(lead.id),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        for (project_id, sentinel) in [
            (allowed, "allowed-export-body"),
            (hidden, "private-export-body"),
        ] {
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: format!("{sentinel}-issue"),
                    description: sentinel.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(project_id),
                    title: format!("{sentinel}-page"),
                    content: sentinel.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        hidden
    };
    let identity = crate::auth::fresh_identity(&viewer, Transport::Web);
    let response = held_export(&db, identity, |db| {
        db.transaction(|tx| {
            tx.execute(
                "UPDATE projects SET identifier='REN' WHERE id=?1",
                [allowed],
            )?;
            tx.execute("UPDATE projects SET identifier='MEM' WHERE id=?1", [hidden])?;
            Ok(())
        })
        .unwrap();
    })
    .await
    .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let bundle: crate::export::ExportBundle = serde_json::from_slice(&body).unwrap();
    let rendered = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        !rendered.contains("private-export-body"),
        "an identifier change must never select the inaccessible project: {rendered}"
    );
    assert!(
        rendered.contains("allowed-export-body-issue"),
        "the selected project issue must be retained: {rendered}"
    );
    assert!(
        rendered.contains("allowed-export-body-page"),
        "the selected project page must be retained: {rendered}"
    );
    assert_eq!(
        bundle.root, "REN",
        "metadata belongs to the same immutable selected project"
    );
}

#[tokio::test]
async fn project_export_rechecks_membership_in_the_worker_snapshot() {
    let (db, _, _, _, viewer, _, selected) = setup_membership_test();
    let identity = crate::auth::fresh_identity(&viewer, Transport::Web);
    let result = held_export(&db, identity, |db| {
        queries::members::remove_member(&db.write().unwrap(), selected, viewer.id).unwrap();
    })
    .await;
    assert!(
        matches!(result, Err(LificError::Forbidden(_))),
        "a revoked viewer must receive no export response"
    );
}

#[tokio::test]
async fn project_export_rechecks_admin_in_the_worker_snapshot() {
    let (db, admin, _, _, _, _, _) = setup_membership_test();
    let identity = crate::auth::fresh_identity(&admin, Transport::Web);
    let result = held_export(&db, identity, |db| {
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
            .unwrap();
    })
    .await;
    assert!(
        matches!(result, Err(LificError::Forbidden(_))),
        "a demoted admin without membership must receive no export response"
    );
}

#[tokio::test]
async fn project_export_rechecks_active_account_in_the_worker_snapshot() {
    let (db, _, _, _, viewer, _, _) = setup_membership_test();
    let identity = crate::auth::fresh_identity(&viewer, Transport::Web);
    let result = held_export(&db, identity, |db| {
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
            .unwrap();
    })
    .await;
    assert!(
        matches!(result, Err(LificError::Forbidden(_))),
        "a disabled caller must receive no export response"
    );
}
