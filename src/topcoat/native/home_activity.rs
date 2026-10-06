//! Home activity text and logical destinations from the original Svelte view.
//! Values remain ordinary text; the view is responsible for escaping markup.

use crate::db::models::{Activity, Project};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ActivityRow {
    pub(crate) actor: String,
    pub(crate) verb: String,
    pub(crate) label: Option<String>,
    pub(crate) destination: Option<String>,
}

pub(crate) fn activity_row(activity: &Activity, projects: &[Project]) -> ActivityRow {
    let actor = super::avatar::display_name(
        activity.actor_display_name.as_deref(),
        activity.actor_username.as_deref(),
        "system",
    )
    .to_owned();
    let verb = match activity.action.as_str() {
        "create" if activity.entity_type == "comment" => "commented on".into(),
        "delete" if activity.entity_type == "comment" => "deleted a comment on".into(),
        "update" if activity.entity_type == "comment" => "edited a comment on".into(),
        "create" => format!("created {}", activity.entity_type),
        "delete" => format!("deleted {}", activity.entity_type),
        "update" => match activity.field.as_deref().filter(|field| !field.is_empty()) {
            Some(field) => format!("changed {field} on"),
            None => "updated".into(),
        },
        "attach" => "labeled".into(),
        "detach" => "unlabeled".into(),
        "link" => "linked".into(),
        "unlink" => "unlinked".into(),
        _ => activity.action.clone(),
    };
    let project = projects
        .iter()
        .find(|project| Some(project.id) == activity.project_id)
        .filter(|project| !project.identifier.is_empty());
    let label = project.map(|project| {
        activity
            .entity_label
            .clone()
            .unwrap_or_else(|| format!("{} #{}", project.identifier, activity.entity_id))
    });
    let destination = project.and_then(|project| {
        let identifier = &project.identifier;
        let issue_label = activity
            .entity_label
            .as_deref()
            .filter(|label| !label.is_empty());
        match activity.entity_type.as_str() {
            "issue" => issue_label.map(|label| format!("/{identifier}/issues/{label}")),
            "page" => Some(format!("/{identifier}/pages/{}", activity.entity_id)),
            "comment" => match (activity.issue_id, issue_label, activity.page_id) {
                (Some(_), Some(label), _) => Some(format!("/{identifier}/issues/{label}")),
                (_, _, Some(page_id)) => Some(format!("/{identifier}/pages/{page_id}")),
                _ => None,
            },
            _ => None,
        }
    });
    ActivityRow {
        actor,
        verb,
        label,
        destination,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity() -> Activity {
        Activity {
            id: 1,
            ts: "2026-10-03T18:00:00Z".into(),
            actor_user_id: Some(2),
            actor_username: Some("alice".into()),
            actor_display_name: Some("Alice".into()),
            actor_is_bot: false,
            transport: "web".into(),
            entity_type: "issue".into(),
            entity_id: 42,
            entity_label: Some("LIF-42".into()),
            project_id: Some(7),
            issue_id: Some(42),
            page_id: None,
            action: "update".into(),
            field: Some("title".into()),
            old_value: None,
            new_value: None,
        }
    }

    fn projects() -> Vec<Project> {
        vec![Project {
            id: 7,
            name: "Lific".into(),
            identifier: "LIF".into(),
            description: String::new(),
            emoji: None,
            lead_user_id: None,
            sort_order: 0,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-03T18:00:00Z".into(),
            is_public: false,
        }]
    }

    #[test]
    fn native_home_activity_preserves_original_verbs_and_field_truthiness() {
        let cases = [
            ("create", "comment", None, "commented on"),
            ("delete", "comment", None, "deleted a comment on"),
            ("update", "comment", Some("content"), "edited a comment on"),
            ("create", "issue", None, "created issue"),
            ("delete", "page", None, "deleted page"),
            ("update", "issue", Some("priority"), "changed priority on"),
            ("update", "issue", Some(""), "updated"),
            ("update", "issue", None, "updated"),
            ("update", "issue", Some(" "), "changed   on"),
            ("attach", "issue", None, "labeled"),
            ("detach", "issue", None, "unlabeled"),
            ("link", "issue", None, "linked"),
            ("unlink", "issue", None, "unlinked"),
            ("custom-action", "issue", None, "custom-action"),
        ];
        for (action, entity_type, field, expected) in cases {
            let mut event = activity();
            event.action = action.into();
            event.entity_type = entity_type.into();
            event.field = field.map(str::to_owned);
            assert_eq!(activity_row(&event, &projects()).verb, expected);
        }
    }

    #[test]
    fn native_home_activity_actor_falls_back_on_empty_but_preserves_whitespace() {
        let cases = [
            (Some("Alice"), Some("alice"), "Alice"),
            (Some(""), Some("alice"), "alice"),
            (None, Some("alice"), "alice"),
            (Some(""), Some(""), "system"),
            (None, None, "system"),
            (Some(" "), Some("alice"), " "),
            (None, Some(" "), " "),
        ];
        for (display, username, expected) in cases {
            let mut event = activity();
            event.actor_display_name = display.map(str::to_owned);
            event.actor_username = username.map(str::to_owned);
            assert_eq!(activity_row(&event, &projects()).actor, expected);
        }
    }

    #[test]
    fn native_home_activity_destinations_follow_entity_and_comment_parent_rules() {
        let cases = [
            (
                "issue",
                Some("LIF-42"),
                Some(42),
                None,
                Some("/LIF/issues/LIF-42"),
            ),
            ("issue", None, Some(42), None, None),
            ("issue", Some(""), Some(42), None, None),
            ("page", None, None, None, Some("/LIF/pages/42")),
            (
                "comment",
                Some("LIF-42"),
                Some(42),
                Some(9),
                Some("/LIF/issues/LIF-42"),
            ),
            ("comment", None, Some(42), Some(9), Some("/LIF/pages/9")),
            ("comment", Some(""), Some(42), Some(9), Some("/LIF/pages/9")),
            (
                "comment",
                Some("LIF-42"),
                None,
                Some(9),
                Some("/LIF/pages/9"),
            ),
            ("comment", None, None, None, None),
            (
                "comment",
                Some("LIF-0"),
                Some(0),
                Some(9),
                Some("/LIF/issues/LIF-0"),
            ),
            ("comment", None, None, Some(0), Some("/LIF/pages/0")),
            (
                "comment",
                Some(" "),
                Some(0),
                Some(0),
                Some("/LIF/issues/ "),
            ),
            ("plan", Some("LIF-PLAN-42"), None, None, None),
        ];
        for (entity_type, label, issue_id, page_id, expected) in cases {
            let mut event = activity();
            event.entity_type = entity_type.into();
            event.entity_label = label.map(str::to_owned);
            event.issue_id = issue_id;
            event.page_id = page_id;
            assert_eq!(
                activity_row(&event, &projects()).destination.as_deref(),
                expected
            );
        }
    }

    #[test]
    fn native_home_activity_unknown_or_null_project_has_no_label_or_destination() {
        for project_id in [None, Some(999)] {
            let mut event = activity();
            event.project_id = project_id;
            let row = activity_row(&event, &projects());
            assert_eq!(row.actor, "Alice");
            assert_eq!(row.verb, "changed title on");
            assert_eq!(row.label, None);
            assert_eq!(row.destination, None);
        }
        assert_eq!(activity_row(&activity(), &[]).destination, None);
        let mut no_identifier = projects();
        no_identifier[0].identifier.clear();
        let row = activity_row(&activity(), &no_identifier);
        assert_eq!(row.label, None);
        assert_eq!(row.destination, None);
    }

    #[test]
    fn native_home_activity_empty_label_is_distinct_from_absent_label() {
        let mut event = activity();
        event.entity_label = Some(String::new());
        let empty = activity_row(&event, &projects());
        assert_eq!(empty.label.as_deref(), Some(""));
        assert_eq!(empty.destination, None);
        event.entity_label = None;
        let absent = activity_row(&event, &projects());
        assert_eq!(absent.label.as_deref(), Some("LIF #42"));
        assert_eq!(absent.destination, None);
        event.entity_label = Some(" ".into());
        let whitespace = activity_row(&event, &projects());
        assert_eq!(whitespace.label.as_deref(), Some(" "));
        assert_eq!(whitespace.destination.as_deref(), Some("/LIF/issues/ "));
    }

    #[test]
    fn native_home_activity_hostile_text_remains_data_for_the_escaping_view() {
        let mut event = activity();
        event.actor_display_name = Some("<img src=x onerror=alert(1)>".into());
        event.action = "<script>alert(2)</script>".into();
        event.entity_type = "unknown".into();
        event.entity_label = Some("<svg onload=alert(3)>".into());
        let row = activity_row(&event, &projects());
        assert_eq!(row.actor, "<img src=x onerror=alert(1)>");
        assert_eq!(row.verb, "<script>alert(2)</script>");
        assert_eq!(row.label.as_deref(), Some("<svg onload=alert(3)>"));
        assert_eq!(row.destination, None);
    }
}
