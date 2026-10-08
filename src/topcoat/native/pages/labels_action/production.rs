//! Real-router contracts for the sparse page-label procedure.

use super::super::super::home_fixture;
use super::{Reply, Request, Snapshot};
use crate::db::{
    models::{CreateLabel, Page, Role, UpdatePage},
    queries,
};
use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

fn seed() -> (home_fixture::Fixture, Request) {
    let fixture = home_fixture::fixture();
    let (page_id, account_id, _) = super::super::production::seed_page(&fixture, true);
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    let conn = fixture.db.write().unwrap();
    for name in ["First label", "Second label", "Concurrent label"] {
        queries::create_label(
            &conn,
            &CreateLabel {
                project_id: page.project_id.unwrap(),
                name: name.to_owned(),
                color: "#16a34a".into(),
            },
        )
        .unwrap();
    }
    drop(conn);
    let request = Request {
        account_id,
        page_id,
        identifier: page.identifier,
        label: "First label".into(),
        attach: true,
    };
    (fixture, request)
}

async fn call(fixture: &home_fixture::Fixture, request: &Request) -> serde_json::Value {
    let (status, reply) = home_fixture::procedure(
        fixture,
        "/__native_pages/labels",
        serde_json::to_value((request.clone(),).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "typed page-label reply: {reply}");
    reply
}

fn canonical_reply(request: &Request, page: &Page) -> serde_json::Value {
    serde_json::to_value(
        Reply {
            status: Ok("saved".into()),
            account_id: request.account_id,
            page_id: page.id,
            canonical: Some(Snapshot {
                identifier: page.identifier.clone(),
                title: page.title.clone(),
                content: page.content.clone(),
                seq: page.seq,
                page_status: page.status.clone(),
                pinned: page.pinned,
                labels: page.labels.clone(),
            }),
        }
        .into_surrogate(),
    )
    .unwrap()
}

fn failure_reply(request: &Request, error: &str) -> serde_json::Value {
    serde_json::to_value(
        Reply {
            status: Err(error.to_owned()),
            account_id: request.account_id,
            page_id: request.page_id,
            canonical: None,
        }
        .into_surrogate(),
    )
    .unwrap()
}

#[tokio::test]
async fn native_page_labels_apply_individual_intent_and_return_exact_canonical_state() {
    let (fixture, request) = seed();
    let initial = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    let first = call(&fixture, &request).await;
    let saved = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    assert_eq!(saved.labels, ["First label"]);
    assert_eq!(first, canonical_reply(&request, &saved));
    assert!(saved.seq > initial.seq);

    // Both intents were formed from the initial document, before either save.
    let second_request = Request {
        label: "Second label".into(),
        ..request.clone()
    };
    let second = call(&fixture, &second_request).await;
    let mut saved = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    assert_eq!(second, canonical_reply(&request, &saved));
    saved.labels.sort();
    assert_eq!(saved.labels, ["First label", "Second label"]);

    let remove = Request {
        attach: false,
        ..request
    };
    let reply = call(&fixture, &remove).await;
    let saved = queries::get_page(&fixture.db.read().unwrap(), remove.page_id).unwrap();
    assert_eq!(saved.labels, ["Second label"]);
    assert_eq!(reply, canonical_reply(&remove, &saved));
}

#[tokio::test]
async fn native_page_labels_preserve_current_labels_and_unrelated_concurrent_metadata() {
    let (fixture, request) = seed();
    {
        let conn = fixture.db.write().unwrap();
        queries::update_page(
            &conn,
            request.page_id,
            &UpdatePage {
                title: Some("Concurrent title".into()),
                content: Some("Concurrent body".into()),
                status: Some("active".into()),
                pinned: Some(true),
                sort_order: Some(19.0),
                labels: Some(vec!["Concurrent label".into()]),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let before = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    let reply = call(&fixture, &request).await;
    let saved = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    assert_eq!(reply, canonical_reply(&request, &saved));
    let mut labels = saved.labels.clone();
    labels.sort();
    assert_eq!(labels, ["Concurrent label", "First label"]);
    assert_eq!(saved.title, before.title);
    assert_eq!(saved.content, before.content);
    assert_eq!(saved.status, before.status);
    assert_eq!(saved.pinned, before.pinned);
    assert_eq!(saved.folder_id, before.folder_id);
    assert_eq!(saved.sort_order, before.sort_order);
}

#[tokio::test]
async fn native_page_labels_recheck_current_role_and_reject_changed_account_or_identity() {
    let (fixture, request) = seed();
    let wrong_account = Request {
        account_id: request.account_id + 100,
        ..request.clone()
    };
    assert_eq!(
        call(&fixture, &wrong_account).await,
        failure_reply(&wrong_account, "Your account changed. Reload this page."),
    );
    let wrong_identity = Request {
        identifier: "ACC-DOC-999999".into(),
        ..request.clone()
    };
    assert_eq!(
        call(&fixture, &wrong_identity).await,
        failure_reply(&wrong_identity, "not found")
    );
    {
        let conn = fixture.db.write().unwrap();
        let page = queries::get_page(&conn, request.page_id).unwrap();
        queries::members::upsert_member(
            &conn,
            page.project_id.unwrap(),
            request.account_id,
            Role::Viewer,
        )
        .unwrap();
    }
    assert_eq!(
        call(&fixture, &request).await,
        failure_reply(
            &request,
            "requires at least 'maintainer' access to this project"
        ),
    );
    assert!(
        queries::get_page(&fixture.db.read().unwrap(), request.page_id)
            .unwrap()
            .labels
            .is_empty()
    );
}

#[tokio::test]
async fn native_page_labels_ignore_unknown_and_other_project_names_like_main_update_page() {
    let (fixture, request) = seed();
    {
        let conn = fixture.db.write().unwrap();
        let hidden = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
        queries::create_label(
            &conn,
            &CreateLabel {
                project_id: hidden,
                name: "Private label".into(),
                color: "#112233".into(),
            },
        )
        .unwrap();
    }
    call(&fixture, &request).await;
    for name in ["Unknown label", "Private label"] {
        let unknown = Request {
            label: name.into(),
            ..request.clone()
        };
        let reply = call(&fixture, &unknown).await;
        let saved = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
        assert_eq!(saved.labels, ["First label"]);
        assert_eq!(reply, canonical_reply(&unknown, &saved));
    }
}

#[tokio::test]
async fn native_page_labels_failed_insert_rolls_back_and_returns_safe_error_without_canonical() {
    let (fixture, request) = seed();
    {
        let conn = fixture.db.write().unwrap();
        queries::update_page(
            &conn,
            request.page_id,
            &UpdatePage {
                labels: Some(vec!["Concurrent label".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute_batch("CREATE TRIGGER reject_page_label BEFORE INSERT ON page_labels BEGIN SELECT RAISE(ABORT, 'private page label diagnostic'); END;").unwrap();
    }
    let before = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    assert_eq!(
        call(&fixture, &request).await,
        failure_reply(&request, "internal server error")
    );
    let saved = queries::get_page(&fixture.db.read().unwrap(), request.page_id).unwrap();
    assert_eq!(saved.labels, before.labels);
    assert_eq!(saved.seq, before.seq);
    assert_eq!(saved.title, before.title);
    assert_eq!(saved.content, before.content);
}
