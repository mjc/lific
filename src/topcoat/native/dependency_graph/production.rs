use axum::http::StatusCode;

use super::super::home_fixture::{self, Fixture};
use crate::db::{
    models::{CreateIssue, ListIssuesQuery, Status},
    queries,
};

fn linked_graph_fixture() -> (Fixture, i64, i64, i64, i64) {
    let fixture = home_fixture::fixture();
    let (active_id, todo_id, backlog_id, done_id) = {
        let conn = fixture.db.write().unwrap();
        let project = queries::list_projects(&conn)
            .unwrap()
            .into_iter()
            .find(|project| project.identifier == "ACC")
            .unwrap();
        let issues = queries::list_issues(
            &conn,
            &ListIssuesQuery {
                project_id: Some(project.id),
                limit: Some(500),
                ..Default::default()
            },
        )
        .unwrap();
        let active = issues
            .iter()
            .find(|issue| issue.status == Status::Active)
            .unwrap();
        let todo = issues
            .iter()
            .find(|issue| issue.status == Status::Todo)
            .unwrap();
        let backlog = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project.id,
                title: "Unlinked backlog work".into(),
                status: Status::Backlog,
                ..Default::default()
            },
        )
        .unwrap();
        let done = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project.id,
                title: "Closed linked work".into(),
                status: Status::Done,
                ..Default::default()
            },
        )
        .unwrap();
        queries::link_issues(&conn, active.id, todo.id, "blocks").unwrap();
        queries::link_issues(&conn, todo.id, done.id, "relates_to").unwrap();
        (active.id, todo.id, backlog.id, done.id)
    };
    (fixture, active_id, todo_id, backlog_id, done_id)
}

#[tokio::test]
async fn production_graph_mount_renders_authorized_initial_canvas_and_readonly_controls() {
    let (fixture, active_id, todo_id, backlog_id, done_id) = linked_graph_fixture();
    for mount in ["", "/app", "/ACC"] {
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            "/ACC/graph?source=production-contract",
            true,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "graph mount {mount}");
        let document = scraper::Html::parse_document(&html);
        let root = scraper::Selector::parse("[data-native-dependency-graph='ACC']").unwrap();
        let graph = document.select(&root).next().expect("native graph root");
        assert_eq!(graph.value().attr("data-editable"), Some("false"));

        let nodes = scraper::Selector::parse("[data-native-graph-node]").unwrap();
        let actual_nodes = document.select(&nodes).collect::<Vec<_>>();
        assert_eq!(
            actual_nodes.len(),
            2,
            "closed and unlinked nodes stay off linked canvas"
        );
        let node = |id: i64| {
            actual_nodes
                .iter()
                .copied()
                .find(|node| {
                    node.value()
                        .attr("data-native-graph-node")
                        .and_then(|value| value.parse::<i64>().ok())
                        == Some(id)
                })
                .unwrap_or_else(|| panic!("missing issue node {id}"))
        };
        let active = node(active_id);
        let todo = node(todo_id);
        assert_eq!(active.value().attr("data-x"), Some("0"));
        assert_eq!(active.value().attr("data-y"), Some("0"));
        assert_eq!(todo.value().attr("data-x"), Some("290"));
        assert_eq!(todo.value().attr("data-y"), Some("0"));
        assert_eq!(active.value().attr("data-width"), Some("200"));
        assert_eq!(active.value().attr("data-height"), Some("58"));
        assert!(actual_nodes.iter().all(|node| {
            node.value()
                .attr("data-native-graph-node")
                .and_then(|value| value.parse::<i64>().ok())
                != Some(backlog_id)
        }));
        assert!(actual_nodes.iter().all(|node| {
            node.value()
                .attr("data-native-graph-node")
                .and_then(|value| value.parse::<i64>().ok())
                != Some(done_id)
        }));
        assert_eq!(
            document
                .select(&scraper::Selector::parse("[data-native-graph-edge]").unwrap())
                .count(),
            1
        );

        let linked = scraper::Selector::parse("[data-native-graph-view='linked']").unwrap();
        assert_eq!(
            document
                .select(&linked)
                .next()
                .unwrap()
                .value()
                .attr("aria-pressed"),
            Some("true")
        );
        let unlinked = scraper::Selector::parse("[data-native-graph-view='unlinked']").unwrap();
        assert!(document.select(&unlinked).next().is_some());
        let closed = scraper::Selector::parse("[data-native-graph-closed]").unwrap();
        assert_eq!(
            document
                .select(&closed)
                .next()
                .unwrap()
                .value()
                .attr("aria-pressed"),
            Some("false")
        );
        assert_eq!(
            document
                .select(&scraper::Selector::parse("[data-native-graph-connect]").unwrap())
                .count(),
            0
        );
        assert_eq!(
            document
                .select(&scraper::Selector::parse("[data-native-graph-manage]").unwrap())
                .count(),
            0
        );
        assert!(
            html.contains("ACC-"),
            "nodes keep issue identifiers and labels"
        );
    }
}

#[tokio::test]
async fn production_graph_relation_procedure_rechecks_viewer_role() {
    let (fixture, active_id, todo_id, _, _) = linked_graph_fixture();
    use topcoat::runtime::Surrogated;
    let (account, project, source, target) = {
        let conn = fixture.db.read().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let project = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let source = queries::get_issue(&conn, active_id).unwrap();
        let target = queries::get_issue(&conn, todo_id).unwrap();
        (account, project, source.identifier, target.identifier)
    };
    let arguments = serde_json::to_value(
        (account, project, source, target, "duplicate".to_owned()).into_surrogate(),
    )
    .unwrap();

    let (status, response) =
        home_fixture::procedure(&fixture, "/__native_dependency_graph/link", arguments).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "viewer cannot create graph links: {response}"
    );
    let project_id = queries::list_projects(&fixture.db.read().unwrap())
        .unwrap()
        .into_iter()
        .find(|project| project.identifier == "ACC")
        .unwrap()
        .id;
    let relations =
        queries::list_project_relations(&fixture.db.read().unwrap(), project_id).unwrap();
    assert_eq!(
        relations.len(),
        2,
        "denied procedure leaves persisted graph unchanged"
    );
}

#[tokio::test]
async fn rendered_zoom_in_handler_updates_the_graph_transform() {
    let (fixture, _, _, _, _) = linked_graph_fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/ACC/graph", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let zoom_in = document
        .select(&scraper::Selector::parse("[data-native-graph-zoom='in']").unwrap())
        .next()
        .unwrap();
    let zoom_out = document
        .select(&scraper::Selector::parse("[data-native-graph-zoom='out']").unwrap())
        .next()
        .unwrap();
    let zoom_controls = document
        .select(&scraper::Selector::parse("[data-native-graph-zoom-controls]").unwrap())
        .next()
        .expect("joined zoom controls");
    assert!(
        zoom_controls
            .value()
            .attr("class")
            .unwrap_or_default()
            .contains("rounded-lg"),
        "zoom buttons share a compact joined surface"
    );
    let controls = document
        .select(&scraper::Selector::parse("[data-native-graph-controls]").unwrap())
        .next()
        .expect("bottom-right control group");
    let controls_class = controls.value().attr("class").unwrap_or_default();
    assert!(controls_class.contains("right-3") && controls_class.contains("bottom-3"));
    assert!(
        zoom_controls
            .select(&scraper::Selector::parse("[data-icon='Minus']").unwrap())
            .next()
            .is_some()
    );
    assert!(
        zoom_controls
            .select(&scraper::Selector::parse("[data-icon='Plus']").unwrap())
            .next()
            .is_some()
    );
    let fit = document
        .select(&scraper::Selector::parse("[data-native-graph-fit]").unwrap())
        .next()
        .unwrap();
    assert!(
        fit.select(&scraper::Selector::parse("[data-icon='Maximize']").unwrap())
            .next()
            .is_some(),
        "Fit uses its own maximize icon"
    );
    let transform = document
        .select(&scraper::Selector::parse("[data-native-graph-transform]").unwrap())
        .next()
        .unwrap();
    let signals = home_fixture::page_signals(&html);
    let actions = ["in", "out"]
        .into_iter()
        .chain(std::iter::repeat_n("in", 10))
        .chain(std::iter::repeat_n("out", 20))
        .chain(["in", "out"])
        .collect::<Vec<_>>();
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/dependency_graph/zoom_handler.test.cjs",
        &serde_json::json!({
            "handlers":[
                zoom_in.value().attr("data-topcoat-on:click"),
                zoom_out.value().attr("data-topcoat-on:click")
            ],
            "actions": actions,
            "viewport":{"width":800,"height":600},
            "style_binding":transform.value().attr("data-topcoat-bind:style"),
            "signals":signals
        }),
    );
    let styles = result["styles"].as_array().unwrap();
    assert!(styles[0].as_str().unwrap().contains("scale:1.2"));
    assert!(
        styles[0]
            .as_str()
            .unwrap()
            .contains("translate:-80px -60px"),
        "zoom anchors to the viewport center: {}",
        styles[0].as_str().unwrap()
    );
    assert!(
        styles[0]
            .as_str()
            .unwrap()
            .contains("translate 0ms,scale 150ms"),
        "zoom transitions scale without animating pan"
    );
    assert!(styles[1].as_str().unwrap().contains("scale:1"));
    assert!(styles[1].as_str().unwrap().contains("translate:0px 0px"));
    assert!(styles[11].as_str().unwrap().contains("scale:2"));
    assert!(styles[31].as_str().unwrap().contains("scale:0.1"));
    assert!(styles[32].as_str().unwrap().contains("scale:0.12"));
    assert!(styles[33].as_str().unwrap().contains("scale:0.1"));
    let first_signals = result["snapshots"][0].as_object().unwrap().clone();
    let (status, updated_html) =
        home_fixture::document(&fixture, "/app", "/ACC/graph", true, Some(first_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let updated = scraper::Html::parse_document(&updated_html);
    let surface = updated
        .select(&scraper::Selector::parse("[data-native-graph-transform]").unwrap())
        .next()
        .unwrap();
    assert!(
        surface
            .value()
            .attr("style")
            .unwrap_or_default()
            .contains("scale:1.2"),
        "zoom-in signal is reflected in rendered graph transform: {updated_html}"
    );
    let second_signals = result["snapshots"][1].as_object().unwrap().clone();
    let (status, updated_html) =
        home_fixture::document(&fixture, "/app", "/ACC/graph", true, Some(second_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let updated = scraper::Html::parse_document(&updated_html);
    let surface = updated
        .select(&scraper::Selector::parse("[data-native-graph-transform]").unwrap())
        .next()
        .unwrap();
    assert!(
        surface
            .value()
            .attr("style")
            .unwrap_or_default()
            .contains("scale:1"),
        "reciprocal zoom-out restores scale 1: {updated_html}"
    );
}

#[tokio::test]
async fn rendered_graph_fit_control_resets_view_and_viewport_supports_pan() {
    let (fixture, _, _, _, _) = linked_graph_fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/ACC/graph", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let fit = document
        .select(&scraper::Selector::parse("[data-native-graph-fit]").unwrap())
        .next()
        .unwrap();
    let viewport = document
        .select(&scraper::Selector::parse("[data-native-graph-pan-surface]").unwrap())
        .next()
        .unwrap();
    let transform = document
        .select(&scraper::Selector::parse("[data-native-graph-transform]").unwrap())
        .next()
        .unwrap();
    let signals = home_fixture::page_signals(&html);
    let transform_style = transform.value().attr("style").unwrap_or_default();
    let dimension = |property: &str| {
        transform_style
            .split(';')
            .find_map(|declaration| declaration.trim().strip_prefix(property))
            .and_then(|value| value.trim().strip_suffix("px"))
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap()
    };
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/dependency_graph/viewport_handler.test.cjs",
        &serde_json::json!({
            "fit_handler": fit.value().attr("data-topcoat-on:click"),
            "pan_mount": viewport.value().attr("data-topcoat-on:mount"),
            "style_binding": transform.value().attr("data-topcoat-bind:style"),
            "signals": signals,
            "width": dimension("width:"),
            "height": dimension("height:")
        }),
    );
    let fit_style = result["fit_style"].as_str().unwrap();
    let padding_x = ((800.0_f64 - 800.0 / 1.15) * 0.5).floor();
    let padding_y = ((600.0_f64 - 600.0 / 1.15) * 0.5).floor();
    let fit_scale = ((800.0 - 2.0 * padding_x) / dimension("width:"))
        .min((600.0 - 2.0 * padding_y) / dimension("height:"))
        .clamp(0.1, 2.0);
    let fit_x = (800.0 - dimension("width:") * fit_scale) / 2.0;
    let fit_y = (600.0 - dimension("height:") * fit_scale) / 2.0;
    let initial_style = result["initial_style"].as_str().unwrap();
    assert!(
        initial_style.contains(&format!("scale:{fit_scale}")),
        "mount autofits graph bounds: {initial_style}"
    );
    assert!(initial_style.contains(&format!("translate:{fit_x}px {fit_y}px")));
    assert!(
        initial_style.contains("translate 0ms,scale 0ms"),
        "initial autofit does not animate"
    );
    assert!(
        fit_style.contains(&format!("scale:{fit_scale}")),
        "fit uses XYFlow's 0.15 padding: {fit_style}"
    );
    assert!(
        fit_style.contains(&format!("translate:{fit_x}px {fit_y}px")),
        "fit centers graph bounds: {fit_style}"
    );
    assert!(
        fit_style.contains("translate 0ms,scale 200ms"),
        "fit transitions scale only: {fit_style}"
    );
    assert_eq!(
        result["guarded_style"].as_str().unwrap(),
        fit_style,
        "interactive targets and secondary inputs do not pan"
    );
    let pan_style = result["pan_style"].as_str().unwrap();
    assert!(
        pan_style.contains(&format!("translate:{}px {}px", fit_x + 20.0, fit_y + 15.0)),
        "pointer drag pans from fitted position: {pan_style}"
    );
    assert_eq!(
        result["cancel_style"].as_str().unwrap(),
        pan_style,
        "pointer cancellation ends the drag"
    );
    assert_ne!(
        result["active_drag_style"].as_str().unwrap(),
        pan_style,
        "a new pointer can start another drag"
    );
    assert_eq!(
        result["drag_active_after_abort"], false,
        "unmount clears active pointer state"
    );
    assert_eq!(
        result["listeners_removed"], true,
        "unmount removes viewport listeners"
    );
    assert_eq!(
        result["cursor_after_abort"], "grab",
        "unmount restores the pan cursor"
    );
    assert_eq!(
        result["remount_idle_style"], result["initial_style"],
        "a new viewport refits and ignores stale pointer movement"
    );
    assert_ne!(
        result["remount_drag_style"], result["remount_idle_style"],
        "a fresh pointer can pan after remount"
    );
}

async fn replayable_graph_document(
    fixture: &Fixture,
    path: &str,
    token: &str,
    signals: Option<serde_json::Map<String, serde_json::Value>>,
) -> (StatusCode, String) {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    let mut request = Request::builder()
        .method(if signals.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("cookie", format!("lific_token={token}"));
    let runtime = signals.is_some();
    let body = if let Some(signals) = signals {
        request = request
            .header("content-type", "application/json")
            .header("x-topcoat-runtime", "true")
            .header("accept", "application/x-ndjson");
        Body::from(serde_json::json!({"signals":signals}).to_string())
    } else {
        Body::empty()
    };
    let mut request = request.body(body).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let html = if runtime && status == StatusCode::OK {
        body.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|frame| frame["t"] == "snapshot")
            .unwrap()["html"]
            .as_str()
            .unwrap()
            .to_owned()
    } else {
        body
    };
    (status, html)
}

#[tokio::test]
async fn graph_replay_state_is_scoped_to_account_and_project() {
    let (fixture, _, _, _, _) = linked_graph_fixture();
    let other_account_token = {
        let conn = fixture.db.write().unwrap();
        let original = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let other_account = queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "graph-other-viewer".into(),
                email: "graph-other-viewer@test.com".into(),
                password: "testpassword1".into(),
                display_name: None,
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        queries::members::upsert_member(
            &conn,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
            other_account.id,
            crate::db::models::Role::Viewer,
        )
        .unwrap();
        queries::create_project(
            &conn,
            &crate::db::models::CreateProject {
                identifier: "OTH".into(),
                name: "Other graph project".into(),
                lead_user_id: Some(original.id),
                ..Default::default()
            },
        )
        .unwrap();
        queries::users::create_session(&conn, other_account.id, None)
            .unwrap()
            .token
    };

    let (status, html) = home_fixture::document(&fixture, "", "/ACC/graph", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let unlinked = document
        .select(&scraper::Selector::parse("[data-native-graph-view='unlinked']").unwrap())
        .next()
        .unwrap();
    let signals = home_fixture::page_signals(&html);
    let changed_signals = home_fixture::evaluate_handler(
        "src/topcoat/native/dependency_graph/replay_handler.test.cjs",
        &serde_json::json!({
            "handler":unlinked.value().attr("data-topcoat-on:click"),
            "signals":signals.clone()
        }),
    );
    let changed_signals = changed_signals.as_object().unwrap().clone();
    assert_eq!(
        changed_signals.len(),
        1,
        "unlinked updates just the view signal"
    );
    let mut replayed_signals = signals.clone();
    replayed_signals.extend(changed_signals);

    for (path, token, expected) in [
        ("/ACC/graph", other_account_token.as_str(), "true"),
        ("/OTH/graph", fixture.token.as_str(), "true"),
        ("/ACC/graph", fixture.token.as_str(), "false"),
    ] {
        let (status, html) =
            replayable_graph_document(&fixture, path, token, Some(replayed_signals.clone())).await;
        assert_eq!(status, StatusCode::OK, "replay target {path}");
        let document = scraper::Html::parse_document(&html);
        let linked = document
            .select(&scraper::Selector::parse("[data-native-graph-view='linked']").unwrap())
            .next()
            .unwrap();
        assert_eq!(
            linked.value().attr("aria-pressed"),
            Some(expected),
            "stale state must reset across account/project changes, and survive for its original owner ({path})"
        );
    }
}
