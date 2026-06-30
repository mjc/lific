use axum::{
    Extension,
    extract::{Json, Path, Query, State},
};

use crate::db::{DbPool, models::*};
use crate::error::LificError;
use crate::realtime::{RealtimeEvent, RealtimeHub};

use super::{with_read, with_write};

pub(super) async fn list_issues(
    State(db): State<DbPool>,
    Query(q): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, LificError> {
    with_read(&db, |conn| crate::db::queries::list_issues(conn, &q)).map(Json)
}

pub(super) async fn get_issue(
    State(db): State<DbPool>,
    Path(id): Path<i64>,
) -> Result<Json<Issue>, LificError> {
    with_read(&db, |conn| crate::db::queries::get_issue(conn, id)).map(Json)
}

pub(super) async fn resolve_issue(
    State(db): State<DbPool>,
    Path(identifier): Path<String>,
) -> Result<Json<Issue>, LificError> {
    with_read(&db, |conn| {
        let id = crate::db::queries::resolve_identifier(conn, &identifier)?;
        crate::db::queries::get_issue(conn, id)
    })
    .map(Json)
}

pub(super) async fn create_issue(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Json(input): Json<CreateIssue>,
) -> Result<Json<Issue>, LificError> {
    let issue = with_write(&db, |conn| crate::db::queries::create_issue(conn, &input))?;
    realtime.send(RealtimeEvent::IssueCreated {
        project_id: issue.project_id,
        issue_id: issue.id,
        identifier: Some(issue.identifier.clone()),
    });
    Ok(Json(issue))
}

pub(super) async fn update_issue(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Path(id): Path<i64>,
    Json(input): Json<UpdateIssue>,
) -> Result<Json<Issue>, LificError> {
    let issue = with_write(&db, |conn| {
        crate::db::queries::update_issue(conn, id, &input)
    })?;
    realtime.send(RealtimeEvent::IssueUpdated {
        project_id: issue.project_id,
        issue_id: issue.id,
        identifier: Some(issue.identifier.clone()),
    });
    Ok(Json(issue))
}

pub(super) async fn delete_issue_handler(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, LificError> {
    let issue = with_read(&db, |conn| crate::db::queries::get_issue(conn, id))?;
    with_write(&db, |conn| crate::db::queries::delete_issue(conn, id))?;
    realtime.send(RealtimeEvent::IssueDeleted {
        project_id: issue.project_id,
        issue_id: issue.id,
        identifier: Some(issue.identifier),
    });
    Ok(Json(serde_json::json!({"deleted": true})))
}

#[derive(serde::Deserialize)]
pub(super) struct LinkRequest {
    source: String,
    target: String,
    relation_type: String,
}

#[derive(serde::Deserialize)]
pub(super) struct UnlinkRequest {
    source: String,
    target: String,
}

pub(super) async fn link_issues(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Json(input): Json<LinkRequest>,
) -> Result<Json<serde_json::Value>, LificError> {
    let (source_id, target_id, source_issue) = with_read(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        let source_issue = crate::db::queries::get_issue(conn, source_id)?;
        Ok((source_id, target_id, source_issue))
    })?;
    with_write(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        crate::db::queries::link_issues(conn, source_id, target_id, &input.relation_type)
    })?;
    realtime.send(RealtimeEvent::IssueLinked {
        project_id: source_issue.project_id,
        issue_id: source_id,
    });
    realtime.send(RealtimeEvent::IssueLinked {
        project_id: source_issue.project_id,
        issue_id: target_id,
    });
    Ok(Json(serde_json::json!({"linked": true})))
}

pub(super) async fn unlink_issues(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Json(input): Json<UnlinkRequest>,
) -> Result<Json<serde_json::Value>, LificError> {
    let (source_id, target_id, source_issue) = with_read(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        let source_issue = crate::db::queries::get_issue(conn, source_id)?;
        Ok((source_id, target_id, source_issue))
    })?;
    with_write(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        crate::db::queries::unlink_issues(conn, source_id, target_id)
    })?;
    realtime.send(RealtimeEvent::IssueUnlinked {
        project_id: source_issue.project_id,
        issue_id: source_id,
    });
    realtime.send(RealtimeEvent::IssueUnlinked {
        project_id: source_issue.project_id,
        issue_id: target_id,
    });
    Ok(Json(serde_json::json!({"unlinked": true})))
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use crate::config::AuthConfig;
    use crate::realtime::{RealtimeEvent, RealtimeHub};
    use axum::http::{Request, StatusCode};
    use axum::{Extension, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app_with_realtime() -> (Router, RealtimeHub) {
        let db = crate::db::open_memory().expect("test db");
        let admin_id = {
            let conn = db.write().unwrap();
            conn.execute(
                "INSERT INTO users (username, email, password_hash, display_name, is_admin, is_bot)
                 VALUES ('test-admin', 'admin@test.local', 'x', 'Test Admin', 1, 0)",
                [],
            )
            .expect("seed test admin");
            conn.last_insert_rowid()
        };
        let hub = RealtimeHub::new();
        let app = crate::api::router(db, &[])
            .layer(Extension(hub.clone()))
            .layer(Extension(AuthConfig {
                allow_signup: true,
                secure_cookies: false,
            }))
            .layer(Extension(Some(crate::db::models::AuthUser {
                id: admin_id,
                username: "test-admin".into(),
                display_name: "Test Admin".into(),
                is_admin: true,
            })));
        (app, hub)
    }

    #[tokio::test]
    async fn issue_crud_lifecycle() {
        let (app, hub) = test_app_with_realtime();
        let (project_id, _) = seed_project(&app).await;
        let mut events = hub.subscribe();

        // Create issue
        let body = serde_json::json!({
            "project_id": project_id,
            "title": "Fix the bug",
            "status": "todo",
            "priority": "high"
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/issues")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let issue: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let issue_id = issue["id"].as_i64().unwrap();
        assert_eq!(issue["identifier"], "TST-1");
        assert_eq!(issue["priority"], "high");
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueCreated {
                project_id,
                issue_id,
                identifier: Some("TST-1".into()),
            }
        );

        // List with filter
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/issues?project_id={project_id}&status=todo"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.len(), 1);

        // Update
        let update = serde_json::json!({"status": "active"});
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/issues/{issue_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&update).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let updated: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(updated["status"], "active");
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueUpdated {
                project_id,
                issue_id,
                identifier: Some("TST-1".into()),
            }
        );

        // Resolve by identifier
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/issues/resolve/TST-1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Delete
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/issues/{issue_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueDeleted {
                project_id,
                issue_id,
                identifier: Some("TST-1".into()),
            }
        );
    }
}
