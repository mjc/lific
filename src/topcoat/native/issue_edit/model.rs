//! Original title/body commit decisions and typed metadata edits.

use crate::{
    db::models::{Issue, UpdateIssue},
    error::LificError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Title,
    Description,
    Status,
    Priority,
}

/// A no-op matches the original controls; a changed patch always carries the
/// sequence observed by this editor. The shared transaction enforces it.
pub(crate) fn edit_patch(
    field: Field,
    value: &str,
    saved: &Issue,
    observed_seq: i64,
) -> Result<Option<UpdateIssue>, LificError> {
    let mut patch = UpdateIssue {
        expected_seq: Some(observed_seq),
        ..Default::default()
    };
    match field {
        Field::Title => {
            let value = value.trim();
            if value.is_empty() || value == saved.title {
                return Ok(None);
            }
            patch.title = Some(value.to_owned());
        }
        Field::Description => {
            if value == saved.description {
                return Ok(None);
            }
            patch.description = Some(value.to_owned());
        }
        Field::Status => {
            let status = value.parse().map_err(LificError::BadRequest)?;
            if status == saved.status {
                return Ok(None);
            }
            patch.status = Some(status);
        }
        Field::Priority => {
            let priority = value.parse().map_err(LificError::BadRequest)?;
            if priority == saved.priority {
                return Ok(None);
            }
            patch.priority = Some(priority);
        }
    }
    Ok(Some(patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        self,
        models::{CreateIssue, CreateProject, Priority, Status},
        queries,
    };

    fn saved() -> Issue {
        let db = db::open_memory().unwrap();
        let conn = db.write().unwrap();
        let project = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "EDIT".into(),
                name: "Issue editing".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project.id,
                title: "Saved title".into(),
                description: "Saved body".into(),
                status: Status::Active,
                priority: Priority::Medium,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn native_issue_edit_title_trims_and_ignores_empty_or_unchanged_commits() {
        let issue = saved();
        for value in ["", " \n\t ", "Saved title", "  Saved title\n"] {
            assert!(
                edit_patch(Field::Title, value, &issue, issue.seq)
                    .unwrap()
                    .is_none()
            );
        }
        let patch = edit_patch(
            Field::Title,
            "  Changed <script> title\n",
            &issue,
            issue.seq,
        )
        .unwrap()
        .unwrap();
        assert_eq!(patch.title.as_deref(), Some("Changed <script> title"));
        assert_eq!(patch.expected_seq, Some(issue.seq));
        assert!(patch.description.is_none());
        assert!(patch.status.is_none());
        assert!(patch.priority.is_none());
    }

    #[test]
    fn native_issue_edit_description_preserves_exact_markdown_and_can_clear_it() {
        let issue = saved();
        assert!(
            edit_patch(Field::Description, &issue.description, &issue, issue.seq)
                .unwrap()
                .is_none()
        );
        for value in [
            "",
            " \n",
            "  **Draft**\n![reference](/api/attachments/owned)\n",
        ] {
            let patch = edit_patch(Field::Description, value, &issue, issue.seq)
                .unwrap()
                .unwrap();
            assert_eq!(patch.description.as_deref(), Some(value));
            assert_eq!(patch.expected_seq, Some(issue.seq));
            assert!(patch.title.is_none());
            assert!(patch.status.is_none());
            assert!(patch.priority.is_none());
        }
    }

    #[test]
    fn native_issue_edit_status_uses_original_choices_and_rejects_invalid_input() {
        let issue = saved();
        for status in [
            Status::Backlog,
            Status::Todo,
            Status::Active,
            Status::Done,
            Status::Cancelled,
        ] {
            let patch = edit_patch(Field::Status, status.as_str(), &issue, issue.seq).unwrap();
            if status == issue.status {
                assert!(patch.is_none());
            } else {
                let patch = patch.unwrap();
                assert_eq!(patch.status, Some(status));
                assert_eq!(patch.expected_seq, Some(issue.seq));
                assert!(patch.title.is_none());
                assert!(patch.description.is_none());
                assert!(patch.priority.is_none());
            }
        }
        assert!(matches!(
            edit_patch(Field::Status, "not-a-status", &issue, issue.seq),
            Err(LificError::BadRequest(_))
        ));
    }

    #[test]
    fn native_issue_edit_priority_uses_original_choices_and_rejects_invalid_input() {
        let issue = saved();
        for priority in [
            Priority::Urgent,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::None,
        ] {
            let patch = edit_patch(Field::Priority, priority.as_str(), &issue, issue.seq).unwrap();
            if priority == issue.priority {
                assert!(patch.is_none());
            } else {
                let patch = patch.unwrap();
                assert_eq!(patch.priority, Some(priority));
                assert_eq!(patch.expected_seq, Some(issue.seq));
                assert!(patch.title.is_none());
                assert!(patch.description.is_none());
                assert!(patch.status.is_none());
            }
        }
        assert!(matches!(
            edit_patch(Field::Priority, "not-a-priority", &issue, issue.seq),
            Err(LificError::BadRequest(_))
        ));
    }

    #[test]
    fn native_issue_edit_patch_preserves_the_stale_editor_sequence() {
        let issue = saved();
        let observed_seq = issue.seq - 1;
        assert_ne!(observed_seq, issue.seq);
        let patch = edit_patch(Field::Title, "Losing editor draft", &issue, observed_seq)
            .unwrap()
            .unwrap();
        assert_eq!(patch.title.as_deref(), Some("Losing editor draft"));
        assert_eq!(patch.expected_seq, Some(observed_seq));
    }
}
