use super::model::{Draft, defaults};
use crate::db::models::{Priority, Status};

#[test]
fn issue_create_draft_uses_valid_query_defaults_and_preserves_the_complete_payload() {
    let defaults = defaults(Some("17"), Some("active"));
    assert_eq!(defaults.module_id, Some(17));
    assert_eq!(defaults.status, Status::Active);

    let draft = Draft {
        project_id: 4,
        title: "  Ship native issue creation  ".into(),
        description: "markdown body".into(),
        status: defaults.status,
        priority: Priority::Urgent,
        module_id: defaults.module_id,
        labels: vec!["launch".into()],
    };

    let input = draft.input().unwrap();
    assert_eq!(input.project_id, 4);
    assert_eq!(input.title, "Ship native issue creation");
    assert_eq!(input.description, "markdown body");
    assert_eq!(input.status, Status::Active);
    assert_eq!(input.priority, Priority::Urgent);
    assert_eq!(input.module_id, Some(17));
    assert_eq!(input.labels, ["launch"]);
}

#[test]
fn issue_create_defaults_ignore_invalid_query_values_and_reject_blank_titles() {
    let defaults = defaults(Some("bad-id"), Some("unknown"));
    assert_eq!(defaults.module_id, None);
    assert_eq!(defaults.status, Status::Backlog);

    let draft = Draft {
        project_id: 4,
        title: " \n\t ".into(),
        description: String::new(),
        status: Status::Backlog,
        priority: Priority::None,
        module_id: None,
        labels: Vec::new(),
    };
    assert!(draft.input().is_err());
}
