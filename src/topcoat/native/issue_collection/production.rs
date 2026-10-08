use super::super::home_fixture;
use super::model::{self, ViewState};
use crate::{
    actor::Transport,
    db::{
        models::{CreateIssue, Status},
        queries,
    },
    services,
};

#[test]
fn native_issue_collection_loader_reads_beyond_five_hundred_and_keeps_main_preview() {
    let (db, _, _, _, viewer, _, project_id) = crate::api::test_helpers::setup_membership_test();
    let target = db
        .transaction(|conn| {
            for number in 0..510 {
                queries::create_issue(
                    conn,
                    &CreateIssue {
                        project_id,
                        title: format!("Ordinary {number}"),
                        ..Default::default()
                    },
                )?;
            }
            queries::create_issue(
                conn,
                &CreateIssue {
                    project_id,
                    title: "Late target".into(),
                    description: "\n  visible preview  \nsecret-body-needle".into(),
                    status: Status::Active,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let loaded = services::issues::list_project_collection(&db, &identity, "MEM").unwrap();
    assert_eq!(
        loaded.issues.len(),
        511,
        "collection filtering must not stop at a REST page limit"
    );
    assert_eq!(
        loaded
            .issues
            .iter()
            .find(|issue| issue.id == target.id)
            .unwrap()
            .description,
        "visible preview"
    );
    let state = ViewState {
        search_query: "Late target".into(),
        ..Default::default()
    };
    let selected = model::select(&loaded, &state, "list");
    assert_eq!(selected.issues.len(), 1);
    assert_eq!(selected.issues[0].id, target.id);
    assert_eq!(selected.count_label, "1 of 511");
    let state = ViewState {
        search_query: "secret-body-needle".into(),
        ..Default::default()
    };
    assert!(model::select(&loaded, &state, "list").issues.is_empty());
    db.transaction(|conn| {
        conn.execute(
            "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
            rusqlite::params![project_id, viewer.id],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(
        services::issues::list_project_collection(&db, &identity, "MEM").is_err(),
        "a captured identity must not retain revoked project access"
    );
}

#[tokio::test]
async fn native_issue_collection_query_routes_ignore_filter_parameters_and_scope_initial_ssr() {
    let fixture = home_fixture::fixture();
    for mount in ["", "/app", "/ACC"] {
        for path in [
            "/ACC/issues?status=done&label=missing",
            "/ACC/board?assignee=me",
        ] {
            let (status, html) = home_fixture::document(&fixture, mount, path, true, None).await;
            assert_eq!(status, axum::http::StatusCode::OK, "{mount}{path}");
            assert!(html.contains("Visible active initial work"));
            assert!(html.contains("Visible todo initial work"));
            assert!(!html.contains("Private hidden initial work"));
            assert!(html.contains(&format!("href=\"{mount}/ACC/issues/ACC-1\"")));
            let (status, _) = home_fixture::document(&fixture, mount, path, false, None).await;
            assert_ne!(status, axum::http::StatusCode::OK);
        }
        let (status, html) =
            home_fixture::document(&fixture, mount, "/HIDE/issues?status=active", true, None).await;
        assert_ne!(status, axum::http::StatusCode::OK);
        assert!(!html.contains("Private hidden initial work"));
    }
}

#[test]
fn native_issue_collection_loader_rechecks_authority_before_returning_catalog_or_rows() {
    let (db, _, _, _, viewer, non_member, project_id) =
        crate::api::test_helpers::setup_membership_test();
    let outsider = Some(crate::auth::fresh_identity(&non_member, Transport::Web));
    assert!(services::issues::list_project_collection(&db, &outsider, "MEM").is_err());
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    assert!(services::issues::list_project_collection(&db, &identity, "MEM").is_ok());
    db.transaction(|conn| {
        conn.execute(
            "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
            rusqlite::params![project_id, viewer.id],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(services::issues::list_project_collection(&db, &identity, "MEM").is_err());
}

#[tokio::test]
async fn native_issue_collection_initial_list_and_board_ssr_include_rows_beyond_five_hundred() {
    let fixture = home_fixture::fixture();
    let target = fixture
        .db
        .transaction(|conn| {
            let project_id = queries::resolve_project_identifier(conn, "ACC")?;
            for number in 0..510 {
                queries::create_issue(
                    conn,
                    &CreateIssue {
                        project_id,
                        title: format!("Collection seed {number}"),
                        ..Default::default()
                    },
                )?;
            }
            queries::create_issue(
                conn,
                &CreateIssue {
                    project_id,
                    title: "Final authorized collection row".into(),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    for (path, selector) in [
        ("/ACC/issues", "[data-native-issue-row]"),
        ("/ACC/board", "[data-native-board-card]"),
    ] {
        let (status, html) = home_fixture::document(&fixture, "/app", path, true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        assert_eq!(
            document
                .select(&scraper::Selector::parse(selector).unwrap())
                .count(),
            513,
            "initial SSR must cover all authorized candidates at {path}"
        );
        assert!(html.contains(&target.title));
        assert!(!html.contains("Private hidden initial work"));
    }
}
