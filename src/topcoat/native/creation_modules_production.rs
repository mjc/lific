//! New issue and module pages through the authenticated production router.

use axum::http::StatusCode;

use super::home_fixture::{self, document, procedure};
use crate::db::{
    models::{CreateModule, Role},
    queries,
};

fn module(fixture: &home_fixture::Fixture, identifier: &str) -> i64 {
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, identifier).unwrap();
    queries::create_module(
        &conn,
        &CreateModule {
            project_id,
            name: format!("{identifier} module milestone"),
            description: "**Native module description**".into(),
            status: "active".into(),
            emoji: None,
        },
    )
    .unwrap()
    .id
}

#[tokio::test]
async fn native_issue_creation_enforces_account_role_payload_and_actor() {
    use topcoat::runtime::Surrogated;
    let fixture = home_fixture::fixture();
    let module = module(&fixture, "ACC");
    let (account, project) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
        )
    };
    let input = |expected_account| {
        serde_json::to_value(
            (
                expected_account,
                project,
                "  Created natively  ".to_owned(),
                "**Complete description**".to_owned(),
                "active".to_owned(),
                "urgent".to_owned(),
                module,
                Vec::<String>::new(),
            )
                .into_surrogate(),
        )
        .unwrap()
    };
    let mut events = fixture.realtime.subscribe();
    let (status, _) = procedure(
        &fixture,
        "/__native_issue_create/create",
        input(account + 1000),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(events.try_recv().is_err());
    let (status, outcome) =
        procedure(&fixture, "/__native_issue_create/create", input(account)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome[0], false, "readonly creation: {outcome}");
    assert!(events.try_recv().is_err());
    queries::members::upsert_member(
        &fixture.db.write().unwrap(),
        project,
        account,
        Role::Maintainer,
    )
    .unwrap();
    let (status, outcome) =
        procedure(&fixture, "/__native_issue_create/create", input(account)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome[0], true, "native creation: {outcome}");
    let issue = {
        let conn = fixture.db.read().unwrap();
        let id = queries::resolve_identifier(&conn, outcome[1].as_str().unwrap()).unwrap();
        let issue = queries::get_issue(&conn, id).unwrap();
        assert_eq!(issue.title, "Created natively");
        assert_eq!(issue.description, "**Complete description**");
        assert_eq!(issue.status, crate::db::models::Status::Active);
        assert_eq!(issue.priority, crate::db::models::Priority::Urgent);
        assert_eq!(issue.module_id, Some(module));
        let actor: (i64, String) = conn.query_row(
            "SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'issue' AND entity_id = ?1 AND action = 'create' ORDER BY id DESC LIMIT 1",
            [id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(actor, (account, "web".to_owned()));
        issue
    };
    let event = events.try_recv().unwrap();
    let axum::extract::ws::Message::Text(message) = &event.message else {
        panic!("issue event must carry a text envelope");
    };
    let envelope: serde_json::Value = serde_json::from_str(message.as_str()).unwrap();
    assert_eq!(envelope["seq"], issue.seq);
    assert!(
        matches!(event.event, crate::realtime::RealtimeEvent::IssueCreated { project_id, issue_id } if project_id == project && issue_id == issue.id)
    );
    assert!(events.try_recv().is_err());
    queries::members::upsert_member(&fixture.db.write().unwrap(), project, account, Role::Viewer)
        .unwrap();
    let (status, outcome) =
        procedure(&fixture, "/__native_issue_create/create", input(account)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome[0], false, "demoted creation: {outcome}");
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn native_module_mutations_enforce_roles_project_location_and_web_actor() {
    use topcoat::runtime::Surrogated;
    let fixture = home_fixture::fixture();
    let module = module(&fixture, "ACC");
    let (account, project, other) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
            queries::resolve_project_identifier(&conn, "HIDE").unwrap(),
        )
    };
    let update = |expected_project, field: &str, value: &str| {
        serde_json::to_value(
            (
                account,
                expected_project,
                module,
                field.to_owned(),
                value.to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap()
    };
    let delete = |expected_project| {
        serde_json::to_value((account, expected_project, module).into_surrogate()).unwrap()
    };
    let create = serde_json::to_value(
        (
            account,
            project,
            "ACC".to_owned(),
            "  Native milestone  ".to_owned(),
            "🚀".to_owned(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let mut events = fixture.realtime.subscribe();
    for (path, arguments) in [
        ("/__native_modules/create", create.clone()),
        (
            "/__native_modules/update",
            update(project, "name", "Denied rename"),
        ),
        ("/__native_modules/delete", delete(project)),
    ] {
        let (status, _) = procedure(&fixture, path, arguments).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "readonly mutation {path}");
        assert!(events.try_recv().is_err());
    }
    {
        let conn = fixture.db.write().unwrap();
        for target in [project, other] {
            queries::members::upsert_member(&conn, target, account, Role::Maintainer).unwrap();
        }
    }
    let (status, outcome) = procedure(&fixture, "/__native_modules/create", create).await;
    assert_eq!(status, StatusCode::OK, "create: {outcome}");
    let created_id = outcome["v"].as_str().unwrap().parse::<i64>().unwrap();
    let created = queries::get_module(&fixture.db.read().unwrap(), created_id).unwrap();
    assert_eq!(created.name, "Native milestone");
    assert_eq!(created.emoji.as_deref(), Some("🚀"));
    assert_eq!(created.status, "active");
    assert!(
        matches!(events.try_recv().unwrap().event, crate::realtime::RealtimeEvent::ProjectUpdated { project_id } if project_id == project)
    );
    assert!(events.try_recv().is_err());
    for (field, value) in [
        ("name", "Renamed natively"),
        ("description", "**Updated markdown**"),
        ("status", "paused"),
        ("emoji", ""),
    ] {
        let (status, outcome) = procedure(
            &fixture,
            "/__native_modules/update",
            update(project, field, value),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "update {field}: {outcome}");
        assert!(
            matches!(events.try_recv().unwrap().event, crate::realtime::RealtimeEvent::ProjectUpdated { project_id } if project_id == project)
        );
        assert!(events.try_recv().is_err());
    }
    {
        let conn = fixture.db.read().unwrap();
        let saved = queries::get_module(&conn, module).unwrap();
        assert_eq!(saved.name, "Renamed natively");
        assert_eq!(saved.description, "**Updated markdown**");
        assert_eq!(saved.status, "paused");
        assert_eq!(saved.emoji, None);
        let actor: (i64, String) = conn.query_row(
            "SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'module' AND entity_id = ?1 AND action = 'update' ORDER BY id DESC LIMIT 1",
            [module], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(actor, (account, "web".to_owned()));
    }
    fixture
        .db
        .write()
        .unwrap()
        .execute(
            "UPDATE modules SET project_id = ?1 WHERE id = ?2",
            [other, module],
        )
        .unwrap();
    for (path, arguments) in [
        (
            "/__native_modules/update",
            update(project, "name", "Stale page rename"),
        ),
        ("/__native_modules/delete", delete(project)),
    ] {
        let (status, _) = procedure(&fixture, path, arguments).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "moved module mutation {path}"
        );
        assert!(events.try_recv().is_err());
        assert_eq!(
            queries::get_module(&fixture.db.read().unwrap(), module)
                .unwrap()
                .name,
            "Renamed natively"
        );
    }
    let (status, _) = procedure(&fixture, "/__native_modules/delete", delete(other)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(queries::get_module(&fixture.db.read().unwrap(), module).is_err());
    assert!(
        matches!(events.try_recv().unwrap().event, crate::realtime::RealtimeEvent::ProjectUpdated { project_id } if project_id == other)
    );
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn native_creation_and_modules_render_readonly_then_editable_at_every_mount() {
    let fixture = home_fixture::fixture();
    let module = module(&fixture, "ACC");
    let (account, project) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
        )
    };
    for role in [Role::Viewer, Role::Maintainer] {
        queries::members::upsert_member(&fixture.db.write().unwrap(), project, account, role)
            .unwrap();
        for mount in ["", "/app", "/ACC"] {
            for path in [
                format!("/ACC/issues/new?module={module}&status=active"),
                "/ACC/modules?tab=all".into(),
                format!("/ACC/modules/{module}"),
            ] {
                let (status, html) = document(&fixture, mount, &path, true, None).await;
                assert_eq!(status, StatusCode::OK, "{mount}{path}");
                let parsed = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse("[data-native-project-authority]").unwrap();
                let marker = parsed
                    .select(&selector)
                    .next()
                    .unwrap_or_else(|| panic!("missing rendered authority at {mount}{path}"));
                let authority: serde_json::Value = serde_json::from_str(
                    marker
                        .value()
                        .attr("data-native-project-authority")
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(authority["project_id"], project);
                assert_eq!(authority["can_edit_content"], role == Role::Maintainer);
                assert_eq!(authority["can_edit_structure"], role == Role::Maintainer);
                assert!(html.contains(&format!("{mount}/__topcoat-app.css?v=")));
                assert!(
                    !home_fixture::page_signals(&html).is_empty(),
                    "initial hydration at {mount}{path}"
                );
                assert!(!html.contains("/api/modules") && !html.contains("/api/issues"));
                assert!(
                    !html.contains("__topcoat-modules.js")
                        && !html.contains("__topcoat-issue-new.js")
                );
                if path.contains("/modules") {
                    assert!(html.contains("ACC module milestone"), "{mount}{path}");
                    let controls =
                        scraper::Selector::parse("input, textarea, select, form").unwrap();
                    assert_eq!(
                        marker.select(&controls).next().is_some(),
                        role == Role::Maintainer,
                        "module edit controls at {mount}{path}"
                    );
                } else if role == Role::Maintainer {
                    assert!(
                        html.contains("Issue title"),
                        "Main title placeholder at {mount}{path}"
                    );
                } else {
                    assert!(
                        marker
                            .text()
                            .collect::<String>()
                            .contains("You can't create issues here")
                    );
                    let controls = scraper::Selector::parse("input, textarea, form").unwrap();
                    assert!(
                        marker.select(&controls).next().is_none(),
                        "readonly issue creation at {mount}{path}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn native_module_save_handlers_keep_the_mounted_destination() {
    use std::{io::Write, process::Stdio};

    let fixture = home_fixture::fixture();
    let module = module(&fixture, "ACC");
    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let project = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::members::upsert_member(&conn, project, user.id, Role::Maintainer).unwrap();
    }
    for mount in ["", "/app", "/ACC"] {
        let path = format!("/ACC/modules/{module}");
        let (status, html) = document(&fixture, mount, &path, true, None).await;
        assert_eq!(status, StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let forms = scraper::Selector::parse("[data-native-module-detail] form").unwrap();
        let handlers = document.select(&forms).map(|form| {
            let field = if form.select(&scraper::Selector::parse("[data-native-module-icon]").unwrap()).next().is_some() {
                "emoji"
            } else if form.select(&scraper::Selector::parse("textarea").unwrap()).next().is_some() {
                "description"
            } else if form.select(&scraper::Selector::parse("select").unwrap()).next().is_some() {
                "status"
            } else {
                "name"
            };
            serde_json::json!({"field":field,"source":form.value().attr("data-topcoat-on:submit").unwrap()})
        }).collect::<Vec<_>>();
        assert_eq!(
            handlers
                .iter()
                .map(|handler| handler["field"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["emoji"],
            "description now uses the separately owned read/edit/preview workflow",
        );
        let mut child = std::process::Command::new("node")
            .arg("src/topcoat/native/module_mutations.test.cjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::json!({
                    "mount":mount,"destination":format!("{mount}{path}"),"handlers":handlers,
                    "signals":home_fixture::page_signals(&html),
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "module handlers at {mount}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let description_owner = document
            .select(&scraper::Selector::parse("[data-native-module-description-owner]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("data-topcoat-on:click")
            .expect("the durable description owner handles Edit and Save");
        let entered = home_fixture::evaluate_handler(
            "src/topcoat/native/modules/description_handlers.test.cjs",
            &serde_json::json!({
                "phase":"enter_edit",
                "mount":mount,
                "signals":home_fixture::page_signals(&html),
                "owner":description_owner,
            }),
        );
        assert_eq!(
            entered["requests"], 0,
            "entering Edit does not save the module"
        );
        let (status, edit_html) = home_fixture::document(
            &fixture,
            mount,
            &path,
            true,
            Some(entered["signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let edit_document = scraper::Html::parse_document(&edit_html);
        let edit_owner = edit_document
            .select(&scraper::Selector::parse("[data-native-module-description-owner]").unwrap())
            .next()
            .unwrap();
        let editor = edit_document
            .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
            .next()
            .expect("the emitted Edit action mounts the description editor");
        let result = home_fixture::evaluate_handler(
            "src/topcoat/native/modules/description_handlers.test.cjs",
            &serde_json::json!({
                "mount":mount,
                "initial_description":"**Native module description**",
                "signals":home_fixture::page_signals(&edit_html),
                "owner":edit_owner.value().attr("data-topcoat-on:click").unwrap(),
                "input":editor.value().attr("data-topcoat-on:input").unwrap(),
            }),
        );
        assert_eq!(
            result["explicit_save_url"],
            format!("{mount}/__native_modules/update"),
            "description Save uses the active mounted procedure at {mount}",
        );
        assert_eq!(result["explicit_save_arguments"][3], "description");
        assert_eq!(result["explicit_save_arguments"][4], "Saved module body");
    }
}

#[tokio::test]
async fn native_creation_and_modules_keep_hydrated_navigation_and_query_at_every_mount() {
    let fixture = home_fixture::fixture();
    let module = module(&fixture, "ACC");
    for mount in ["", "/app", "/ACC"] {
        let (_, initial) = document(&fixture, mount, "/", true, None).await;
        let mut signals = home_fixture::page_signals(&initial);
        for path in [
            format!("/ACC/issues/new?module={module}&status=active"),
            "/ACC/modules?tab=all".into(),
            format!("/ACC/modules/{module}"),
            "/".into(),
        ] {
            let (status, html) = document(&fixture, mount, &path, true, Some(signals)).await;
            assert_eq!(status, StatusCode::OK, "{mount}{path}");
            assert!(
                html.contains("native-home-shell"),
                "shared chrome at {mount}{path}"
            );
            signals = home_fixture::page_signals(&html);
        }
    }
}

#[tokio::test]
async fn native_creation_and_modules_reject_hidden_resources_and_revoked_membership() {
    let fixture = home_fixture::fixture();
    let visible = module(&fixture, "ACC");
    let hidden = module(&fixture, "HIDE");
    for path in [
        "/ACC/issues/new".into(),
        "/ACC/modules".into(),
        format!("/ACC/modules/{visible}"),
    ] {
        let (status, _) = document(&fixture, "", &path, false, None).await;
        assert!(status.is_redirection(), "{path}: {status}");
    }
    for path in ["/HIDE/modules".into(), format!("/ACC/modules/{hidden}")] {
        let (_, html) = document(&fixture, "", &path, true, None).await;
        assert!(
            !html.contains("HIDE module milestone"),
            "hidden module at {path}"
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("DELETE FROM project_members WHERE user_id = ?1", [user.id])
            .unwrap();
    }
    for path in [
        "/ACC/issues/new".into(),
        "/ACC/modules".into(),
        format!("/ACC/modules/{visible}"),
    ] {
        let (status, html) = document(&fixture, "", &path, true, None).await;
        if path == "/ACC/issues/new" {
            assert!(
                html.contains("Couldn't load this project"),
                "Main unavailable state at {path}"
            );
            let parsed = scraper::Html::parse_document(&html);
            let composer = scraper::Selector::parse("input[placeholder='Issue title']").unwrap();
            assert!(
                parsed.select(&composer).next().is_none(),
                "revoked issue composer at {path}"
            );
        } else {
            assert_ne!(status, StatusCode::OK, "revoked route at {path}");
        }
        assert!(
            !html.contains("ACC module milestone"),
            "revoked module at {path}"
        );
    }
}
