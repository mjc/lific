use axum::{
    Extension,
    extract::{Json, Path, Query, State},
};

use crate::authz;
use crate::db::queries::plans;
use crate::db::{DbPool, models::*};
use crate::error::LificError;
use crate::realtime::RealtimeHub;

use super::{filter_visible, with_read};

pub(super) async fn list_plans(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Query(q): Query<ListPlansQuery>,
) -> Result<Json<Vec<Plan>>, LificError> {
    if let Some(pid) = q.project_id {
        authz::require_role(&db, &identity, pid, Role::Viewer)?;
        return with_read(&db, |conn| plans::list_plans(conn, &q)).map(Json);
    }
    // Cross-project list (LIF-197 scope item 2): filter, don't deny.
    let visible = authz::visible_project_ids(&db, &identity)?;
    let list = with_read(&db, |conn| plans::list_plans(conn, &q))?;
    Ok(Json(filter_visible(list, &visible, |p| Some(p.project_id))))
}

pub(super) async fn get_plan(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(id): Path<i64>,
) -> Result<Json<Plan>, LificError> {
    crate::services::plans::get(&db, &identity, id).map(Json)
}

pub(super) async fn resolve_plan(
    State(db): State<DbPool>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(identifier): Path<String>,
) -> Result<Json<Plan>, LificError> {
    let plan = with_read(&db, |conn| {
        let id = plans::resolve_plan_identifier(conn, &identifier)?;
        plans::get_plan(conn, id)
    })?;
    authz::require_role(&db, &identity, plan.project_id, Role::Viewer)?;
    Ok(Json(plan))
}

pub(super) async fn create_plan(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Json(input): Json<CreatePlan>,
) -> Result<Json<Plan>, LificError> {
    crate::services::plans::create(&db, &realtime, &identity, &input).map(Json)
}

pub(super) async fn update_plan(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(id): Path<i64>,
    Json(input): Json<UpdatePlan>,
) -> Result<Json<Plan>, LificError> {
    crate::services::plans::update(&db, &realtime, &identity, id, &input).map(Json)
}

pub(super) async fn delete_plan_handler(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, LificError> {
    crate::services::plans::delete(&db, &realtime, &identity, id)?;
    Ok(Json(serde_json::json!({"deleted": true})))
}

pub(super) async fn add_step(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path(plan_id): Path<i64>,
    Json(input): Json<crate::services::plans::AddStep>,
) -> Result<Json<Plan>, LificError> {
    crate::services::plans::add_step(&db, &realtime, &identity, plan_id, &input).map(Json)
}

pub(super) async fn update_step(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path((plan_id, step_id)): Path<(i64, i64)>,
    Json(input): Json<crate::services::plans::UpdateStep>,
) -> Result<Json<crate::services::plans::StepUpdate>, LificError> {
    crate::services::plans::update_step(&db, &realtime, &identity, plan_id, step_id, &input)
        .map(Json)
}

pub(super) async fn delete_step_handler(
    State(db): State<DbPool>,
    Extension(realtime): Extension<RealtimeHub>,
    Extension(identity): Extension<Option<crate::resolve_caller::ResolvedIdentity>>,
    Path((plan_id, step_id)): Path<(i64, i64)>,
) -> Result<Json<Plan>, LificError> {
    crate::services::plans::delete_step(&db, &realtime, &identity, plan_id, step_id).map(Json)
}

#[cfg(test)]
mod tests {
    use crate::api::test_helpers::*;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn plan_crud_and_step_cascade() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;

        // Seed an issue to mirror.
        let issue = json_post(
            &app,
            "/api/issues",
            serde_json::json!({"project_id": project_id, "title": "Work", "status": "todo"}),
        )
        .await;
        let issue = body_json(issue).await;
        let issue_id = issue["id"].as_i64().unwrap();

        // Create a plan with one issue-linked step.
        let resp = json_post(
            &app,
            "/api/plans",
            serde_json::json!({
                "project_id": project_id,
                "title": "Ship it",
                "steps": [{"title": "mirror", "issue_id": issue_id}]
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let plan = body_json(resp).await;
        let plan_id = plan["id"].as_i64().unwrap();
        assert_eq!(plan["identifier"], "TST-PLAN-1");
        let step_id = plan["steps"][0]["id"].as_i64().unwrap();

        // Mark the step done → issue should close, effect reported.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/plans/{plan_id}/steps/{step_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({"done": true})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let out = body_json(resp).await;
        assert_eq!(out["effect"]["issue_status_changed"], true);
        assert_eq!(out["plan"]["steps"][0]["done"], true);

        // List filtered by status.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/plans?project_id={project_id}&status=active"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list = body_json(resp).await;
        assert_eq!(list.as_array().unwrap().len(), 1);

        // Delete.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/plans/{plan_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn plan_list_id_cursor_pages_without_duplicates_and_rejects_invalid_ordering() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;
        for title in ["First", "Second", "Third"] {
            let response = json_post(
                &app,
                "/api/plans",
                serde_json::json!({"project_id": project_id, "title": title}),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        let first = body_json(
            json_get(
                &app,
                &format!("/api/plans?project_id={project_id}&order_by=id&limit=2"),
            )
            .await,
        )
        .await;
        let first = first.as_array().unwrap();
        assert_eq!(first.len(), 2);
        let cursor_id = first[1]["id"].as_i64().unwrap();

        let response = json_get(
            &app,
            &format!(
                "/api/plans?project_id={project_id}&order_by=id&limit=2&before_id={cursor_id}"
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let second = body_json(response).await;
        let second = second.as_array().unwrap();
        assert_eq!(second.len(), 1);
        let second_id = second[0]["id"].as_i64().unwrap();
        assert!(
            !first
                .iter()
                .any(|plan| plan["id"].as_i64().unwrap() == second_id)
        );

        for query in [
            format!("/api/plans?project_id={project_id}&before_id={cursor_id}"),
            format!("/api/plans?project_id={project_id}&order_by=updated&before_id={cursor_id}"),
            format!("/api/plans?project_id={project_id}&order_by=created"),
        ] {
            let response = json_get(&app, &query).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn completing_cross_project_linked_step_emits_issue_updated_for_issue_project() {
        let test = test_app_with_realtime();
        let (plan_project_id, _) = seed_project(&test.app).await;
        let issue_project = body_json(
            json_post(
                &test.app,
                "/api/projects",
                serde_json::json!({
                    "name": "Issue Project",
                    "identifier": "ISS",
                }),
            )
            .await,
        )
        .await;
        let issue_project_id = issue_project["id"].as_i64().unwrap();
        let issue = body_json(
            json_post(
                &test.app,
                "/api/issues",
                serde_json::json!({
                    "project_id": issue_project_id,
                    "title": "Cross-project issue",
                }),
            )
            .await,
        )
        .await;
        let issue_id = issue["id"].as_i64().unwrap();
        let plan = body_json(
            json_post(
                &test.app,
                "/api/plans",
                serde_json::json!({
                    "project_id": plan_project_id,
                    "title": "Cross-project plan",
                    "steps": [{"title": "Complete linked issue", "issue_id": issue_id}],
                }),
            )
            .await,
        )
        .await;
        let plan_id = plan["id"].as_i64().unwrap();
        let step_id = plan["steps"][0]["id"].as_i64().unwrap();
        let mut events = test.realtime.subscribe();

        let resp = test
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/plans/{plan_id}/steps/{step_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({"done": true})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let project_event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        let axum::extract::ws::Message::Text(project_text) = project_event.message else {
            panic!("expected text realtime event");
        };
        let project_event: serde_json::Value = serde_json::from_str(&project_text).unwrap();
        assert_eq!(project_event["type"], "project.updated");
        assert_eq!(project_event["project_id"], plan_project_id);

        let issue_event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        let axum::extract::ws::Message::Text(issue_text) = issue_event.message else {
            panic!("expected text realtime event");
        };
        let issue_event: serde_json::Value = serde_json::from_str(&issue_text).unwrap();
        assert_eq!(issue_event["type"], "issue.updated");
        assert_eq!(issue_event["project_id"], issue_project_id);
        assert_eq!(issue_event["issue_id"], issue_id);
    }

    /// LIF-407: the plan's own project was the only thing gating a step's
    /// `done` toggle, but the toggle closes the *linked issue*, which can
    /// live anywhere. A maintainer on the plan's project with no role on the
    /// issue's project must be refused, and the issue must stay open — this
    /// is the check MCP has had since LIF-198 and REST did not.
    #[tokio::test]
    async fn completing_a_step_linked_to_a_foreign_projects_issue_is_forbidden() {
        let (db, _admin, _lead, maintainer, _viewer, _non_member, plan_project_id) =
            setup_membership_test();

        let (plan_id, step_id, foreign_issue_id) = {
            let conn = db.write().unwrap();
            let foreign = crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "Foreign".into(),
                    identifier: "FGN".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let issue = crate::db::queries::create_issue(
                &conn,
                &crate::db::models::CreateIssue {
                    project_id: foreign.id,
                    title: "Not yours".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let plan = crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id: plan_project_id,
                    title: "Reaches across".into(),
                    issue_id: None,
                    steps: vec![crate::db::models::CreatePlanStep {
                        title: "mirror".into(),
                        description: String::new(),
                        issue_id: Some(issue.id),
                        done: false,
                        steps: vec![],
                    }],
                },
            )
            .unwrap();
            (plan.id, plan.steps[0].id, issue.id)
        };

        let app = app_as_user(db.clone(), &maintainer);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/plans/{plan_id}/steps/{step_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({"done": true})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "completing a step that closes another project's issue must be refused"
        );

        let status = {
            let conn = db.read().unwrap();
            crate::db::queries::get_issue(&conn, foreign_issue_id)
                .unwrap()
                .status
        };
        assert_ne!(
            status.as_str(),
            "done",
            "the foreign issue must not have been closed"
        );
    }

    /// LIF-407: the attach side of the same edge — linking a step to an issue
    /// in a project the caller has no role on is refused too.
    #[tokio::test]
    async fn attaching_a_foreign_projects_issue_to_a_step_is_forbidden() {
        let (db, _admin, _lead, maintainer, _viewer, _non_member, plan_project_id) =
            setup_membership_test();

        let (plan_id, step_id, foreign_issue_id) = {
            let conn = db.write().unwrap();
            let foreign = crate::db::queries::create_project(
                &conn,
                &crate::db::models::CreateProject {
                    name: "Foreign".into(),
                    identifier: "FGN".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let issue = crate::db::queries::create_issue(
                &conn,
                &crate::db::models::CreateIssue {
                    project_id: foreign.id,
                    title: "Not yours".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let plan = crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id: plan_project_id,
                    title: "Attach target".into(),
                    issue_id: None,
                    steps: vec![crate::db::models::CreatePlanStep {
                        title: "step".into(),
                        description: String::new(),
                        issue_id: None,
                        done: false,
                        steps: vec![],
                    }],
                },
            )
            .unwrap();
            (plan.id, plan.steps[0].id, issue.id)
        };

        let app = app_as_user(db.clone(), &maintainer);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/plans/{plan_id}/steps/{step_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({"issue_id": foreign_issue_id}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let linked = {
            let conn = db.read().unwrap();
            crate::db::queries::plans::step_issue_id(&conn, step_id).unwrap()
        };
        assert_eq!(linked, None, "the step must not have been linked");
    }

    /// LIF-407: `parent_step_id` was inserted verbatim, so a step id from a
    /// different plan grafted the new step into that plan's tree.
    #[tokio::test]
    async fn add_step_rejects_a_parent_step_from_another_plan() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;

        let mut plans = Vec::new();
        for title in ["Plan A", "Plan B"] {
            let plan = body_json(
                json_post(
                    &app,
                    "/api/plans",
                    serde_json::json!({
                        "project_id": project_id,
                        "title": title,
                        "steps": [{"title": "root"}]
                    }),
                )
                .await,
            )
            .await;
            plans.push((
                plan["id"].as_i64().unwrap(),
                plan["steps"][0]["id"].as_i64().unwrap(),
            ));
        }
        let (plan_a, _a_step) = plans[0];
        let (plan_b, b_step) = plans[1];

        let resp = json_post(
            &app,
            &format!("/api/plans/{plan_a}/steps"),
            serde_json::json!({"title": "smuggled", "parent_step_id": b_step}),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "a parent step from another plan must be rejected"
        );

        // Plan B's tree is untouched.
        let plan_b_after = body_json(json_get(&app, &format!("/api/plans/{plan_b}")).await).await;
        assert_eq!(plan_b_after["steps"].as_array().unwrap().len(), 1);
        assert!(
            plan_b_after["steps"][0]["steps"]
                .as_array()
                .is_none_or(|children| children.is_empty()),
            "nothing may have been grafted under the foreign step: {plan_b_after}"
        );

        // The same call with the plan's own step still works.
        let resp = json_post(
            &app,
            &format!("/api/plans/{plan_a}/steps"),
            serde_json::json!({"title": "legit", "parent_step_id": plans[0].1}),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn step_description_edit_and_plan_activity() {
        let app = test_app();
        let (project_id, _) = seed_project(&app).await;

        let plan = body_json(
            json_post(
                &app,
                "/api/plans",
                serde_json::json!({
                    "project_id": project_id,
                    "title": "Doc plan",
                    "steps": [{"title": "step"}]
                }),
            )
            .await,
        )
        .await;
        let plan_id = plan["id"].as_i64().unwrap();
        let step_id = plan["steps"][0]["id"].as_i64().unwrap();

        // Set the step's description (the previously-missing capability).
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/plans/{plan_id}/steps/{step_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({"description": "the body"}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let out = body_json(resp).await;
        assert_eq!(out["plan"]["steps"][0]["description"], "the body");

        // Plan activity feed includes plan + step rows.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/plans/{plan_id}/activity"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let feed = body_json(resp).await;
        let items = feed["items"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|a| a["entity_type"] == "plan" && a["action"] == "create")
        );
        assert!(
            items
                .iter()
                .any(|a| a["entity_type"] == "plan_step" && a["field"] == "description"),
            "step description edit must show in plan activity: {feed}"
        );
    }
}
