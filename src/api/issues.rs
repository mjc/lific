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
        identifier: issue.identifier.clone(),
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
        identifier: issue.identifier.clone(),
    });
    Ok(Json(issue))
}

pub(super) async fn delete_issue_handler(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, LificError> {
    let issue = with_write(&db, |conn| {
        let issue = crate::db::queries::get_issue(conn, id)?;
        crate::db::queries::delete_issue(conn, id)?;
        Ok(issue)
    })?;
    realtime.send(RealtimeEvent::IssueDeleted {
        project_id: issue.project_id,
        issue_id: issue.id,
        identifier: issue.identifier,
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
    let (source_event, target_event) = with_write(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        let source = crate::db::queries::get_issue(conn, source_id)?;
        let target = crate::db::queries::get_issue(conn, target_id)?;
        crate::db::queries::link_issues(conn, source_id, target_id, &input.relation_type)?;
        Ok((
            RealtimeEvent::IssueLinked {
                project_id: source.project_id,
                issue_id: source.id,
            },
            RealtimeEvent::IssueLinked {
                project_id: target.project_id,
                issue_id: target.id,
            },
        ))
    })?;
    realtime.send(source_event);
    realtime.send(target_event);
    Ok(Json(serde_json::json!({"linked": true})))
}

pub(super) async fn unlink_issues(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Json(input): Json<UnlinkRequest>,
) -> Result<Json<serde_json::Value>, LificError> {
    let (source_event, target_event) = with_write(&db, |conn| {
        let source_id = crate::db::queries::resolve_identifier(conn, &input.source)?;
        let target_id = crate::db::queries::resolve_identifier(conn, &input.target)?;
        let source = crate::db::queries::get_issue(conn, source_id)?;
        let target = crate::db::queries::get_issue(conn, target_id)?;
        crate::db::queries::unlink_issues(conn, source_id, target_id)?;
        Ok((
            RealtimeEvent::IssueUnlinked {
                project_id: source.project_id,
                issue_id: source.id,
            },
            RealtimeEvent::IssueUnlinked {
                project_id: target.project_id,
                issue_id: target.id,
            },
        ))
    })?;
    realtime.send(source_event);
    realtime.send(target_event);
    Ok(Json(serde_json::json!({"unlinked": true})))
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use crate::realtime::RealtimeEvent;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn issue_crud_lifecycle() {
        let RealtimeTestApp { app, realtime: hub } = test_app_with_realtime();
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
                identifier: "TST-1".into(),
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
                identifier: "TST-1".into(),
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
                identifier: "TST-1".into(),
            }
        );
    }

    #[tokio::test]
    async fn relation_events_use_each_issue_project_id() {
        let RealtimeTestApp { app, realtime: hub } = test_app_with_realtime();
        let (source_project_id, _) = seed_project(&app).await;
        let target_project = serde_json::json!({
            "name": "Target Project",
            "identifier": "TGT",
            "description": "cross-project relation target"
        });
        let resp = json_post(&app, "/api/projects", target_project).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let target_project = parse_json(resp).await;
        let target_project_id = target_project["id"].as_i64().unwrap();

        let source = json_post(
            &app,
            "/api/issues",
            serde_json::json!({
                "project_id": source_project_id,
                "title": "Source",
            }),
        )
        .await;
        assert_eq!(source.status(), StatusCode::OK);
        let source = parse_json(source).await;
        let source_issue_id = source["id"].as_i64().unwrap();

        let target = json_post(
            &app,
            "/api/issues",
            serde_json::json!({
                "project_id": target_project_id,
                "title": "Target",
            }),
        )
        .await;
        assert_eq!(target.status(), StatusCode::OK);
        let target = parse_json(target).await;
        let target_issue_id = target["id"].as_i64().unwrap();

        let mut events = hub.subscribe();
        let resp = json_post(
            &app,
            "/api/issues/link",
            serde_json::json!({
                "source": "TST-1",
                "target": "TGT-1",
                "relation_type": "relates_to",
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueLinked {
                project_id: source_project_id,
                issue_id: source_issue_id,
            }
        );
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueLinked {
                project_id: target_project_id,
                issue_id: target_issue_id,
            }
        );

        let resp = json_post(
            &app,
            "/api/issues/unlink",
            serde_json::json!({
                "source": "TST-1",
                "target": "TGT-1",
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueUnlinked {
                project_id: source_project_id,
                issue_id: source_issue_id,
            }
        );
        assert_eq!(
            events.recv().await.unwrap(),
            RealtimeEvent::IssueUnlinked {
                project_id: target_project_id,
                issue_id: target_issue_id,
            }
        );
    }
}
