//! Real production overview/browser boundaries, with no application substitutes.
use super::super::home_fixture;
use crate::db::{models::CreateProjectGroup, queries};

#[tokio::test]
async fn native_overview_real_browser_roles_inline_drafts_labels_publication_and_disclosures_at_every_mount()
 {
    let fixture = home_fixture::fixture();
    let viewer_token = fixture.token.clone();
    let (admin, token, project, original_lead, group) = {
        let conn = fixture.db.write().unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        let token = queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token;
        let project = queries::list_projects(&conn)
            .unwrap()
            .into_iter()
            .find(|project| project.identifier == "ACC")
            .unwrap();
        let group = queries::project_groups::create_group(
            &conn,
            admin.id,
            &CreateProjectGroup {
                name: "Overview smoke filing".into(),
            },
        )
        .unwrap();
        (admin.id, token, project.id, project.lead_user_id, group.id)
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_overview/overview.smoke.test.cjs"
        ),
        &origin,
        &token,
    );
    command.arg(
        serde_json::json!({"viewer":viewer_token,"project":project,"group":group}).to_string(),
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), command.output()).await;
    server.abort();
    let output = result
        .expect("Native overview real browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "Native overview real controls:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let conn = fixture.db.read().unwrap();
    let saved = queries::get_project(&conn, project).unwrap();
    assert_eq!(saved.name, "Overview saved 2");
    assert_eq!(saved.description, "Preserved overview description 2");
    assert_eq!(saved.identifier, "ACC");
    assert_eq!(saved.lead_user_id, original_lead);
    assert!(!saved.is_public);
    let labels = queries::list_labels(&conn, project).unwrap();
    for index in 0..3 {
        assert_eq!(
            labels
                .iter()
                .filter(|label| label.name == format!("Smoke label {index}"))
                .count(),
            1
        );
    }
    assert_eq!(labels.len(), 3);
    let groups = queries::project_groups::list_groups(&conn, admin).unwrap();
    assert!(
        groups
            .iter()
            .any(|value| value.id == group && value.project_ids.contains(&project))
    );
    let project_edits: i64 = conn.query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='update' AND actor_user_id=?2 AND transport='web'", rusqlite::params![project,admin], |row|row.get(0)).unwrap();
    assert_eq!(
        project_edits, 12,
        "Three real name edits, three description edits and six publication changes."
    );
}

#[tokio::test]
async fn native_overview_matches_pinned_master_paired_identity_sections_and_disclosures() {
    let fixture = home_fixture::fixture();
    let (token, project) = {
        let conn = fixture.db.write().unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        let token = queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token;
        queries::project_groups::create_group(
            &conn,
            admin.id,
            &CreateProjectGroup {
                name: "Overview visual filing".into(),
            },
        )
        .unwrap();
        let project = queries::list_projects(&conn)
            .unwrap()
            .into_iter()
            .find(|project| project.identifier == "ACC")
            .unwrap()
            .id;
        (token, project)
    };
    // The shared fixture renames MEM before any browser is served. Retain
    // its genuine system audit rows so zero browser mutations are observable.
    let project_updates = |conn: &rusqlite::Connection| {
        conn.prepare(
            "SELECT id, actor_user_id, transport, field, old_value, new_value
             FROM audit_log WHERE entity_type='project' AND entity_id=?1
             AND action='update' ORDER BY id",
        )
        .unwrap()
        .query_map([project], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let audit_before = project_updates(&fixture.db.read().unwrap());
    assert_eq!(
        audit_before
            .iter()
            .map(|row| (
                row.1,
                row.2.as_str(),
                row.3.as_deref(),
                row.4.as_deref(),
                row.5.as_deref(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                None,
                "system",
                Some("name"),
                Some("Membership Test"),
                Some("Visible project")
            ),
            (None, "system", Some("identifier"), Some("MEM"), Some("ACC")),
        ],
        "Before serving the browser, the fixture has only its two exact identity changes."
    );
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_overview/overview.geometry.test.cjs"
        ),
        &origin,
        &token,
    );
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command
        .arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").expect("Pinned master web source required"));
    let result = tokio::time::timeout(std::time::Duration::from_secs(300), command.output()).await;
    server.abort();
    let output = result
        .expect("Overview paired real browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "Overview paired visual requirements:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let conn = fixture.db.read().unwrap();
    let saved = queries::get_project(&conn, project).unwrap();
    assert_eq!(saved.name, "Visible project");
    assert_eq!(saved.description, "");
    assert_eq!(saved.identifier, "ACC");
    assert!(!saved.is_public);
    assert!(queries::list_labels(&conn, project).unwrap().is_empty());
    let audit_after = project_updates(&conn);
    let audit_cursor = audit_before.last().map_or(0, |row| row.0);
    let writes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='project'
             AND entity_id=?1 AND action='update' AND id>?2",
            rusqlite::params![project, audit_cursor],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        writes, 0,
        "Visual drafts, canceled identity edits and acknowledgements perform no project mutations. Before: {audit_before:?}; after: {audit_after:?}"
    );
    assert_eq!(
        audit_after, audit_before,
        "Visual interaction preserves every original project audit record."
    );
}
