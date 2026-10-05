use super::*;
use crate::db::{
    models::{CreateIssue, CreateProject},
    queries,
};

fn fixture() -> (Project, Vec<Issue>) {
    let db = crate::db::open_memory().unwrap();
    let conn = db.write().unwrap();
    let project = queries::create_project(
        &conn,
        &CreateProject {
            name: "Overview".into(),
            identifier: "OVR".into(),
            description: "Saved description".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let issues = (0..9)
        .map(|index| {
            let mut issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: project.id,
                    title: format!("Issue {index}"),
                    status: Status::Todo,
                    priority: Priority::Medium,
                    ..Default::default()
                },
            )
            .unwrap();
            issue.created_at = "2026-09-01 00:00:00".into();
            issue.updated_at = "2026-09-20 00:00:00".into();
            issue
        })
        .collect();
    (project, issues)
}
const NOW: i64 = 1_791_072_000_000; // 2026-10-04T00:00:00Z

#[test]
fn overview_attention_uses_weight_age_idle_status_and_excludes_closed() {
    let (_, mut issues) = fixture();
    issues[0].priority = Priority::Urgent;
    issues[1].priority = Priority::High;
    issues[2].status = Status::Done;
    issues[2].priority = Priority::Urgent;
    issues[3].status = Status::Cancelled;
    issues[3].priority = Priority::Urgent;
    issues[4].priority = Priority::None;
    issues[4].created_at = "2025-01-01 00:00:00".into();
    issues[4].updated_at = issues[4].created_at.clone();
    assert!(importance(&issues[4], NOW) > importance(&issues[0], NOW));
    let result = attention(&issues, NOW);
    assert_eq!(result.issues.len(), 6);
    assert_eq!(result.more, 1);
    assert_eq!(result.issues[0].id, issues[4].id);
    assert_eq!(result.issues[1].id, issues[0].id);
    assert_eq!(result.issues[2].id, issues[1].id);
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| matches!(issue.status, Status::Done | Status::Cancelled))
    );
}
#[test]
fn overview_equal_scores_retain_database_order_and_top_six() {
    let (_, issues) = fixture();
    let result = attention(&issues, NOW);
    assert_eq!(
        result.issues.iter().map(|i| i.id).collect::<Vec<_>>(),
        issues[..6].iter().map(|i| i.id).collect::<Vec<_>>()
    );
    assert_eq!(result.more, 3);
    assert_eq!(attention(&[], NOW).more, 0);
}
#[test]
fn overview_date_age_floor_future_invalid_and_month_rounding() {
    assert_eq!(days_since("2026-10-03 00:00:00", NOW), 1);
    assert_eq!(days_since("2026-10-03T00:00:00", NOW), 1);
    assert_eq!(days_since("2026-10-03 00:00:00.001", NOW), 0);
    assert_eq!(days_since("2026-10-05 00:00:00", NOW), 0);
    assert_eq!(days_since("invalid", NOW), 0);
    assert_eq!(
        [0, 1, 59, 60, 74, 75].map(age_label),
        ["today", "1d", "59d", "2mo", "2mo", "3mo"]
    );
}
#[test]
fn overview_publication_is_independent_of_legacy_manage_gate() {
    assert_eq!(
        capabilities(false, false, None, false),
        Capabilities {
            edit: true,
            manage: true,
            publish: false
        }
    );
    assert_eq!(
        capabilities(true, false, Some(Role::Viewer), false),
        Capabilities {
            edit: false,
            manage: false,
            publish: false
        }
    );
    assert_eq!(
        capabilities(true, false, Some(Role::Maintainer), false),
        Capabilities {
            edit: true,
            manage: false,
            publish: false
        }
    );
    assert!(capabilities(false, false, None, true).publish);
    assert!(capabilities(true, false, Some(Role::Lead), false).publish);
    assert!(capabilities(true, true, None, false).manage);
}
#[test]
fn overview_name_description_identifier_and_null_icon_keep_master_commit_rules() {
    let (saved, _) = fixture();
    assert!(field_patch(Field::Name, " \n ", &saved).is_none());
    assert!(field_patch(Field::Name, " Overview ", &saved).is_none());
    assert_eq!(
        field_patch(Field::Name, "\u{FEFF}Renamed\u{FEFF}", &saved)
            .unwrap()
            .name
            .as_deref(),
        Some("Renamed")
    );
    assert_eq!(
        field_patch(Field::Description, "   ", &saved)
            .unwrap()
            .description
            .as_deref(),
        Some("")
    );
    assert_eq!(
        field_patch(Field::Name, "\u{85}Name\u{85}", &saved)
            .unwrap()
            .name
            .as_deref(),
        Some("\u{85}Name\u{85}")
    );
    assert!(field_patch(Field::Identifier, " ovr ", &saved).is_none());
    assert_eq!(
        field_patch(Field::Identifier, " next ", &saved)
            .unwrap()
            .identifier
            .as_deref(),
        Some("NEXT")
    );
    assert_eq!(
        field_patch(Field::Emoji, "", &saved).unwrap().emoji,
        Some(None)
    );
}
#[test]
fn overview_warning_detail_wins_and_missing_expired_or_repeat_still_warns() {
    assert_eq!(
        notice_message(Some("Selected group could not be saved".into()), true).as_deref(),
        Some("Selected group could not be saved")
    );
    assert_eq!(notice_message(None, true).as_deref(), Some(GROUP_WARNING));
    assert_eq!(
        notice_message(Some(String::new()), true).as_deref(),
        Some(GROUP_WARNING)
    );
    assert_eq!(notice_message(None, false), None);
}
#[test]
fn overview_activity_text_preserves_exact_master_verbs_labels_and_bot_fallback() {
    let (project, _) = fixture();
    let db = crate::db::open_memory().unwrap();
    let conn = db.write().unwrap();
    let project = queries::create_project(
        &conn,
        &CreateProject {
            name: project.name,
            identifier: project.identifier,
            ..Default::default()
        },
    )
    .unwrap();
    let mut event = queries::activity::list_activity(
        &conn,
        queries::activity::ActivityScope::Project(project.id),
        Some(14),
        Some(0),
    )
    .unwrap()
    .items
    .remove(0);
    event.actor_username = Some("".into());
    event.actor_display_name = Some("".into());
    event.actor_is_bot = true;
    assert_eq!(actor_name(&event), "a bot");
    event.actor_display_name = Some(" ".into());
    assert_eq!(actor_name(&event), " ");
    event.entity_type = "issue".into();
    event.entity_label = Some("OVR-1".into());
    for (action, expected) in [
        ("create", "created issue OVR-1"),
        ("update", "updated issue OVR-1"),
        ("delete", "deleted issue OVR-1"),
        ("attach", "attach issue OVR-1"),
    ] {
        event.action = action.into();
        assert_eq!(activity_text(&event), expected);
    }
    event.entity_label = Some("".into());
    assert_eq!(activity_text(&event), "attach issue");
}

#[test]
fn overview_notice_query_accepts_only_single_opaque_lowercase_hex_handle() {
    let handle = "a1".repeat(24);
    assert_eq!(
        query_notice(&format!("notice={handle}&group_warning=1")),
        Some(handle.as_str())
    );
    for query in [
        format!("notice={handle}&notice={handle}"),
        format!("notice={}", "A1".repeat(24)),
        format!("notice={}", "a".repeat(47)),
        "notice=%3Cscript%3E".into(),
        "group_warning=1".into(),
    ] {
        assert_eq!(query_notice(&query), None);
    }
}
