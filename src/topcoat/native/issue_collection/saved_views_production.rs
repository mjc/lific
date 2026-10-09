//! Saved-view procedures through the authenticated native router.

use axum::http::StatusCode;

use super::super::home_fixture::{self, procedure};
use crate::db::{models::Role, queries};

fn project_account(fixture: &home_fixture::Fixture) -> (i64, i64) {
    let conn = fixture.db.read().unwrap();
    let account = queries::users::validate_session(&conn, &fixture.token)
        .unwrap()
        .id;
    let project = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    (account, project)
}

fn assert_targeted_event(event: crate::realtime::RealtimeMessage, project_id: i64, user_id: i64) {
    assert!(
        matches!(event.event, crate::realtime::RealtimeEvent::ProjectUpdated { project_id: actual } if actual == project_id)
    );
    let audience = format!("{event:?}");
    assert!(
        audience.contains(&format!("Users([{user_id}])")),
        "event must target only owner {user_id}: {audience}"
    );
}

fn create_args(account: i64, project: i64, name: &str, is_board: bool) -> serde_json::Value {
    use topcoat::runtime::Surrogated;
    let mut config = serde_json::json!({"version":1,"layout":if is_board {"board"} else {"list"},"filterStatus":"active"});
    if is_board {
        config["laneBy"] = "module".into();
        config["hiddenStatuses"] = serde_json::json!(["cancelled"]);
    }
    serde_json::to_value(
        (
            account,
            project,
            name.to_owned(),
            config.to_string(),
            if is_board { "module" } else { "none" }.to_owned(),
            if is_board { "[\"cancelled\"]" } else { "[]" }.to_owned(),
            if is_board {
                "board".to_owned()
            } else {
                "list".to_owned()
            },
        )
            .into_surrogate(),
    )
    .unwrap()
}

fn update_args(
    account: i64,
    project: i64,
    view: i64,
    name: Option<String>,
    default: Option<bool>,
) -> serde_json::Value {
    use topcoat::runtime::Surrogated;
    serde_json::to_value(
        (
            account,
            project,
            view,
            name,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            default,
        )
            .into_surrogate(),
    )
    .unwrap()
}

fn delete_args(account: i64, project: i64, view: i64) -> serde_json::Value {
    use topcoat::runtime::Surrogated;
    serde_json::to_value((account, project, view).into_surrogate()).unwrap()
}

#[tokio::test]
async fn native_saved_view_crud_is_account_and_role_scoped_and_notifies_only_after_commit() {
    let fixture = home_fixture::fixture();
    let (account, project) = project_account(&fixture);
    let mut events = fixture.realtime.subscribe();

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/create",
        create_args(account + 1000, project, "wrong account", false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(events.try_recv().is_err());

    let (status, created) = procedure(
        &fixture,
        "/__native_issue_views/create",
        create_args(account, project, "Working", true),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create response: {created}");
    let view = {
        let conn = fixture.db.read().unwrap();
        queries::views::list_views(&conn, project, account)
            .unwrap()
            .into_iter()
            .find(|view| view.name == "Working")
            .expect("authorized owner can read the new saved view")
    };
    assert_targeted_event(events.try_recv().unwrap(), project, account);

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/create",
        create_args(account, project, "Working", false),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "domain conflicts are returned in the native outcome record"
    );
    assert!(
        events.try_recv().is_err(),
        "a failed duplicate create cannot emit an update event"
    );
    assert_eq!(
        queries::views::list_views(&fixture.db.read().unwrap(), project, account)
            .unwrap()
            .len(),
        1,
        "a failed duplicate create leaves the saved-view list unchanged"
    );

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/update",
        update_args(
            account,
            project,
            view.id,
            Some("Renamed".into()),
            Some(true),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_targeted_event(events.try_recv().unwrap(), project, account);
    {
        let conn = fixture.db.read().unwrap();
        let saved = queries::views::get_owned_view(&conn, view.id, project, account).unwrap();
        assert_eq!(saved.name, "Renamed");
        assert!(saved.is_default);
        let config: serde_json::Value = serde_json::from_str(&saved.config).unwrap();
        assert_eq!(config["layout"], "board");
        assert_eq!(config["laneBy"], "module");
        assert_eq!(config["hiddenStatuses"], serde_json::json!(["cancelled"]));
    }

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/create",
        create_args(account, project, "Second", false),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_targeted_event(events.try_recv().unwrap(), project, account);
    let second = {
        let conn = fixture.db.read().unwrap();
        queries::views::list_views(&conn, project, account)
            .unwrap()
            .into_iter()
            .find(|view| view.name == "Second")
            .unwrap()
    };
    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/update",
        update_args(account, project, second.id, None, Some(true)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_targeted_event(events.try_recv().unwrap(), project, account);
    {
        let conn = fixture.db.read().unwrap();
        assert!(
            !queries::views::get_owned_view(&conn, view.id, project, account)
                .unwrap()
                .is_default
        );
        assert!(
            queries::views::get_owned_view(&conn, second.id, project, account)
                .unwrap()
                .is_default
        );
    }

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/update",
        update_args(account, project, view.id, Some("Second".into()), Some(true)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(events.try_recv().is_err(), "failed writes do not publish");
    {
        let conn = fixture.db.read().unwrap();
        let unchanged = queries::views::get_owned_view(&conn, view.id, project, account).unwrap();
        assert_eq!(unchanged.name, "Renamed");
        assert!(!unchanged.is_default);
        assert!(
            queries::views::get_owned_view(&conn, second.id, project, account)
                .unwrap()
                .is_default,
            "a duplicate-name failure rolls back the default-view switch"
        );
    }

    let (status, _) = procedure(
        &fixture,
        "/__native_issue_views/delete",
        delete_args(account, project, view.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_targeted_event(events.try_recv().unwrap(), project, account);

    queries::members::upsert_member(&fixture.db.write().unwrap(), project, account, Role::Viewer)
        .unwrap();
    fixture
        .db
        .transaction(|conn| {
            conn.execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project, account],
            )?;
            Ok(())
        })
        .unwrap();
    use topcoat::runtime::Surrogated;
    let list = serde_json::to_value((account, project).into_surrogate()).unwrap();
    let (status, _) = procedure(&fixture, "/__native_issue_views/list", list).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "revoked membership denies subsequent reads"
    );
}

#[tokio::test]
async fn emitted_saved_view_mount_installs_dismissal_before_rejection_and_respects_disposal() {
    use scraper::Selector;

    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/issues", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let root = document
        .select(&Selector::parse("[data-native-saved-views]").unwrap())
        .next()
        .expect("saved-view owner is rendered into the native collection");
    let mount_handler = root
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the native owner emits its mount handler");
    let toggle = document
        .select(&Selector::parse("[aria-label='Saved views']").unwrap())
        .next()
        .unwrap();
    let create = document
        .select(&Selector::parse("[data-native-issue-control='saved-view-create']").unwrap())
        .next()
        .unwrap();
    let name = document
        .select(&Selector::parse("#native-saved-view-name").unwrap())
        .next()
        .unwrap();
    let submit = document
        .select(&Selector::parse("#native-saved-view-submit").unwrap())
        .next()
        .unwrap();
    let busy_binding = submit.value().attr("data-topcoat-bind:disabled").unwrap();
    let busy_signal_id = busy_binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("busy binding points at its native signal");
    let signals = home_fixture::page_signals(&html);
    let input = serde_json::json!({
        "signals": signals,
        "mount_handler": mount_handler,
        "toggle_handler": toggle.value().attr("data-topcoat-on:click").unwrap(),
        "create_handler": create.value().attr("data-topcoat-on:click").unwrap(),
        "name_handler": name.value().attr("data-topcoat-on:input").unwrap(),
        "submit_handler": submit.value().attr("data-topcoat-on:click").unwrap(),
        "busy_signal_id": busy_signal_id,
    });
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/issue_collection/saved_views_handler.test.cjs",
        &input,
    );
    assert_eq!(result["rejected_requests"], 1);
    assert_eq!(result["listeners_after_rejection"], 2);
    assert_eq!(result["disposed_requests"], 1);
    assert_eq!(result["disposed_listeners"], 0);
}

#[tokio::test]
async fn native_saved_view_database_failures_keep_internal_details_private() {
    let fixture = home_fixture::fixture();
    let (account, project) = project_account(&fixture);
    fixture
        .db
        .write()
        .unwrap()
        .execute_batch("DROP TABLE saved_views")
        .unwrap();
    let (status, reply) = procedure(
        &fixture,
        "/__native_issue_views/create",
        create_args(account, project, "Working", false),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let encoded = reply.to_string();
    assert!(
        encoded.contains("internal server error"),
        "safe error reply: {encoded}"
    );
    assert!(!encoded.contains("saved_views"));
    assert!(!encoded.contains("Database error"));
}
