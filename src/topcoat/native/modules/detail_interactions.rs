use super::super::super::home_fixture;
use crate::db::{
    models::{CreateModule, Role},
    queries,
};

#[tokio::test]
async fn native_module_detail_maintainer_name_has_inline_edit_trigger() {
    let fixture = home_fixture::fixture();
    let (module_id, name) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Inline edit module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (module.id, module.name)
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, html) = home_fixture::document(&fixture, "", &path, true, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let buttons = scraper::Selector::parse("[data-native-module-detail] button").unwrap();
    assert!(
        document
            .select(&buttons)
            .any(|button| button.text().collect::<String>().trim() == name),
        "Maintainer module name is an inline edit trigger in the actual production view: {html}"
    );
}

#[tokio::test]
async fn native_module_detail_maintainer_status_has_immediate_picker() {
    let fixture = home_fixture::fixture();
    let module_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Status picker module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        module.id
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, html) = home_fixture::document(&fixture, "", &path, true, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let buttons = scraper::Selector::parse("[data-native-module-detail] button").unwrap();
    assert!(
        document
            .select(&buttons)
            .any(|button| button.text().collect::<String>().trim() == "Active"),
        "Maintainer module status is an immediate picker trigger in the actual production view: {html}"
    );
}
