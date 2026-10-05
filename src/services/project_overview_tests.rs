use super::*;
use crate::{
    actor::Transport,
    db::models::{CreateIssue, CreateProject, CreateProjectGroup, Status},
};

fn fixture() -> (
    DbPool,
    crate::db::models::User,
    Option<ResolvedIdentity>,
    i64,
) {
    let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET identifier='OVR' WHERE id=?1",
            [project],
        )
        .unwrap();
    let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    (db, viewer, identity, project)
}
#[test]
fn overview_reads_preserve_project_counts_capped_issues_activity_and_personal_groups() {
    let (db, viewer, identity, project_id) = fixture();
    let hidden_id = {
        let conn = db.write().unwrap();
        let hidden = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "HIDE".into(),
                name: "Private".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let group = queries::project_groups::create_group(
            &conn,
            viewer.id,
            &CreateProjectGroup {
                name: "My group".into(),
            },
        )
        .unwrap();
        queries::project_groups::assign_project(&conn, viewer.id, project_id, Some(group.id))
            .unwrap();
        // A stored hidden group membership is filtered from the response.
        queries::project_groups::assign_project(&conn, viewer.id, hidden.id, Some(group.id))
            .unwrap();
        for index in 0..501 {
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: format!("Work {index}"),
                    status: if index == 500 {
                        Status::Done
                    } else {
                        Status::Todo
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        hidden.id
    };
    queries::settings::update(
        &db.write().unwrap(),
        queries::settings::InstanceSettingsPatch {
            web_auto_login: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let reads = load(&db, &identity, "OVR").unwrap();
    assert!(!reads.web_auto_login);
    assert!(reads.pages.as_ref().unwrap().is_empty());
    assert!(
        reads
            .members
            .as_ref()
            .unwrap()
            .iter()
            .any(|member| member.user_id == viewer.id)
    );
    assert!(
        reads
            .leads
            .as_ref()
            .unwrap()
            .iter()
            .any(|lead| lead.id == viewer.id)
    );
    assert_eq!(reads.project.id, project_id);
    assert_eq!(reads.user.id, viewer.id);
    assert_eq!(reads.role, Some(Role::Viewer));
    assert!(reads.enforced);
    assert!(!reads.projects.iter().any(|project| project.id == hidden_id));
    let counts = reads.counts.unwrap();
    assert_eq!((counts.total, counts.done), (501, 1));
    assert_eq!(
        reads.issues.unwrap().len(),
        queries::MAX_PAGE_LIMIT as usize
    );
    assert_eq!(reads.activity.unwrap().len(), 14);
    let groups = reads.groups.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].project_ids, [project_id]);
}
#[test]
fn overview_hidden_absent_and_differently_cased_identifiers_do_not_load() {
    let (db, _, identity, _) = fixture();
    {
        let conn = db.write().unwrap();
        queries::create_project(
            &conn,
            &CreateProject {
                identifier: "HIDE".into(),
                name: "Hidden".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    for identifier in ["HIDE", "MISS", "ovr"] {
        assert!(matches!(
            load(&db, &identity, identifier),
            Err(LificError::NotFound(_))
        ));
    }
    assert!(matches!(
        load(&db, &None, "OVR"),
        Err(LificError::Forbidden(_))
    ));
}
#[test]
fn overview_retained_identity_observes_disabled_account_and_removed_membership() {
    let (db, viewer, identity, project_id) = fixture();
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
        .unwrap();
    assert!(matches!(
        load(&db, &identity, "OVR"),
        Err(LificError::Forbidden(_))
    ));
    {
        let conn = db.write().unwrap();
        conn.execute("UPDATE users SET is_active=1 WHERE id=?1", [viewer.id])
            .unwrap();
        queries::members::remove_member(&conn, project_id, viewer.id).unwrap();
    }
    assert!(matches!(
        load(&db, &identity, "OVR"),
        Err(LificError::NotFound(_))
    ));
}
#[test]
fn overview_secondary_database_failure_is_explicit_not_an_empty_success() {
    let (db, _, identity, _) = fixture();
    db.write()
        .unwrap()
        .execute("ALTER TABLE labels RENAME TO unavailable_labels", [])
        .unwrap();
    let reads = load(&db, &identity, "OVR").unwrap();
    assert!(matches!(reads.labels, Err(LificError::Database(_))));
    assert!(reads.counts.is_ok());
    assert!(reads.issues.is_ok());
}
