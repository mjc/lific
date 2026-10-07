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
