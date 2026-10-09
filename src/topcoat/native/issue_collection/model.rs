//! Shared, server-side issue list and board selection.
use std::collections::HashMap;

use super::super::super::runtime::whitespace::trim_ecmascript;
use super::super::fuzzy;
use crate::db::{
    models::{Issue, Priority, Status},
    queries::assignees::Assignment,
};

use super::data::Collection;

pub(crate) const STATUSES: [Status; 5] = [
    Status::Backlog,
    Status::Todo,
    Status::Active,
    Status::Done,
    Status::Cancelled,
];
pub(crate) const PRIORITIES: [Priority; 5] = [
    Priority::Urgent,
    Priority::High,
    Priority::Medium,
    Priority::Low,
    Priority::None,
];

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ViewState {
    pub filter_status: String,
    pub filter_priority: String,
    pub filter_label: String,
    pub filter_module: String,
    pub filter_assignee: String,
    pub search_query: String,
    pub sort_field: String,
    pub sort_dir: String,
    pub group_by: String,
    pub density: String,
    pub issue_sub_tab: String,
    pub lane_by: String,
    pub hidden_statuses: Vec<String>,
    pub collapsed_groups: Vec<String>,
    pub collapsed_lanes: Vec<String>,
    pub collapsed_columns: Vec<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            filter_status: String::new(),
            filter_priority: String::new(),
            filter_label: String::new(),
            filter_module: String::new(),
            filter_assignee: String::new(),
            search_query: String::new(),
            sort_field: "priority".into(),
            sort_dir: "asc".into(),
            group_by: "status".into(),
            density: "compact".into(),
            issue_sub_tab: "all".into(),
            lane_by: "none".into(),
            hidden_statuses: Vec::new(),
            collapsed_groups: Vec::new(),
            collapsed_lanes: Vec::new(),
            collapsed_columns: Vec::new(),
        }
    }
}

impl ViewState {
    pub(super) fn active_filter_count(&self) -> usize {
        [
            &self.filter_status,
            &self.filter_priority,
            &self.filter_label,
            &self.filter_module,
            &self.filter_assignee,
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .count()
    }

    pub(super) fn has_active_filters(&self) -> bool {
        self.active_filter_count() > 0
    }
}

#[derive(Debug, Default)]
pub(super) struct Stats {
    pub total: usize,
    pub statuses: [usize; 5],
}

#[derive(Debug)]
pub(crate) struct Group {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub issues: Vec<Issue>,
    pub collapsed: bool,
}

#[derive(Debug)]
pub(crate) struct Lane {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub issues: Vec<Issue>,
    pub collapsed: bool,
}

#[derive(Debug)]
pub(crate) struct Selection {
    pub issues: Vec<Issue>,
    pub groups: Option<Vec<Group>>,
    pub lanes: Option<Vec<Lane>>,
    pub visible_statuses: Vec<Status>,
    pub count_label: String,
    pub show_search_cap: bool,
    pub empty_filtered: bool,
    pub layout: String,
    pub density: String,
    pub collapsed_columns: Vec<String>,
    pub snippets: HashMap<i64, String>,
}

pub(crate) fn select(collection: &Collection, state: &ViewState, layout: &str) -> Selection {
    let stats = statistics(&collection.issues);
    let query = trim_ecmascript(&state.search_query);
    let searching = !query.is_empty();
    let module_id = collection
        .modules
        .iter()
        .find(|module| module.name == state.filter_module)
        .map(|module| module.id);
    let mut hits = collection
        .issues
        .iter()
        .filter(|issue| {
            (state.filter_status.is_empty()
                || if state.filter_status == "@unresolved" {
                    !matches!(issue.status, Status::Done | Status::Cancelled)
                } else {
                    issue.status.as_str() == state.filter_status
                })
                && (state.filter_priority.is_empty()
                    || issue.priority.as_str() == state.filter_priority)
                && (state.filter_label.is_empty() || issue.labels.contains(&state.filter_label))
                && (state.filter_module.is_empty() || issue.module_id == module_id)
                && assignee_matches(collection, issue.id, &state.filter_assignee)
        })
        .filter_map(|issue| {
            if !searching {
                return Some((0.0, issue.clone(), None));
            }
            let title = fuzzy::score(query, &issue.title).unwrap_or(0.0);
            let identifier = fuzzy::score(query, &issue.identifier).unwrap_or(0.0) * 0.9;
            let preview = String::from_utf16_lossy(
                &issue
                    .description
                    .encode_utf16()
                    .take(4000)
                    .collect::<Vec<_>>(),
            );
            let body = fuzzy::find(query, &preview);
            let body_score = body.map_or(0.0, |matched| matched.score * 0.6);
            let score = title.max(identifier).max(body_score);
            if score < 0.25 {
                return None;
            }
            let snippet = body
                .filter(|_| body_score == score && score > 0.0)
                .map(|matched| fuzzy::snippet(&preview, matched));
            Some((score, issue.clone(), snippet))
        })
        .collect::<Vec<_>>();
    if searching {
        // Main caps the stable relevance ranking before its final identifier tie break.
        hits.sort_by(|left, right| right.0.total_cmp(&left.0));
        hits.truncate(50);
        hits.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.identifier.cmp(&right.1.identifier))
        });
    } else {
        hits.sort_by(|left, right| compare_issues(&left.1, &right.1, state));
    }
    let mut snippets = hits
        .iter()
        .filter_map(|(_, issue, snippet)| snippet.clone().map(|snippet| (issue.id, snippet)))
        .collect::<HashMap<_, _>>();
    let mut issues = hits
        .into_iter()
        .map(|(_, issue, _)| issue)
        .collect::<Vec<_>>();
    if layout == "list" {
        match state.issue_sub_tab.as_str() {
            "recent" => {
                issues.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                issues.truncate(20);
            }
            "open" => {
                issues.retain(|issue| !matches!(issue.status, Status::Done | Status::Cancelled))
            }
            "closed" => {
                issues.retain(|issue| matches!(issue.status, Status::Done | Status::Cancelled))
            }
            _ => {}
        }
    }
    snippets.retain(|id, _| issues.iter().any(|issue| issue.id == *id));
    let narrowed =
        state.has_active_filters() || searching || layout == "list" && state.issue_sub_tab != "all";
    let count_label = if narrowed && issues.len() != stats.total {
        format!("{} of {}", issues.len(), stats.total)
    } else {
        stats.total.to_string()
    };
    let groups = if layout == "list" {
        groups(collection, &issues, state, searching)
    } else {
        None
    };
    let lanes = if layout == "board" {
        lanes(collection, &issues, state)
    } else {
        None
    };
    let visible_statuses = STATUSES
        .into_iter()
        .filter(|status| {
            // Preserve Main's literal predicate, including its unresolved sentinel inconsistency.
            (state.filter_status.is_empty() || state.filter_status == status.as_str())
                && !state
                    .hidden_statuses
                    .iter()
                    .any(|hidden| hidden == status.as_str())
        })
        .collect();
    Selection {
        show_search_cap: layout == "list"
            && searching
            && state.issue_sub_tab == "all"
            && issues.len() == 50,
        empty_filtered: state.has_active_filters()
            || !state.search_query.is_empty()
            || layout == "list" && state.issue_sub_tab != "all",
        issues,
        groups,
        lanes,
        visible_statuses,
        count_label,
        layout: layout.to_owned(),
        density: state.density.clone(),
        collapsed_columns: state.collapsed_columns.clone(),
        snippets,
    }
}

fn assignee_matches(collection: &Collection, issue_id: i64, filter: &str) -> bool {
    assignee_filter_matches(
        collection.assignments.get(&issue_id),
        collection.current_user_id,
        filter,
    )
}

fn assignee_filter_matches(
    assignment: Option<&Assignment>,
    current_user_id: i64,
    filter: &str,
) -> bool {
    if filter.is_empty() {
        return true;
    }
    match filter {
        "none" => assignment.is_none(),
        "human" => assignment.is_some_and(|value| value.needs_human),
        "me" => assignment.is_some_and(|value| {
            value
                .assignees
                .iter()
                .any(|person| person.user_id == current_user_id)
        }),
        _ => {
            let Some(username) = filter.strip_prefix('@') else {
                return true;
            };
            if username.is_empty() {
                return true;
            }
            assignment.is_some_and(|value| {
                value
                    .assignees
                    .iter()
                    .any(|person| person.username.eq_ignore_ascii_case(username))
            })
        }
    }
}

pub(super) fn statistics(issues: &[Issue]) -> Stats {
    let mut stats = Stats {
        total: issues.len(),
        ..Default::default()
    };
    for issue in issues {
        if let Some(index) = STATUSES.iter().position(|status| *status == issue.status) {
            stats.statuses[index] += 1;
        }
    }
    stats
}

fn priority_rank(priority: Priority) -> usize {
    match priority {
        Priority::Urgent => 0,
        Priority::High => 1,
        Priority::Medium => 2,
        Priority::Low => 3,
        Priority::None => 4,
    }
}

fn compare_issues(left: &Issue, right: &Issue, state: &ViewState) -> std::cmp::Ordering {
    let result = match state.sort_field.as_str() {
        "priority" => priority_rank(left.priority)
            .cmp(&priority_rank(right.priority))
            .then_with(|| right.created_at.cmp(&left.created_at)),
        "age" => left.created_at.cmp(&right.created_at),
        "updated" => left.updated_at.cmp(&right.updated_at),
        "number" => left.sequence.cmp(&right.sequence),
        _ => std::cmp::Ordering::Equal,
    };
    if state.sort_dir == "asc" {
        result
    } else {
        result.reverse()
    }
}

fn buckets(
    collection: &Collection,
    issues: &[Issue],
    kind: &str,
) -> Vec<(String, String, Vec<Issue>)> {
    match kind {
        "status" => STATUSES
            .into_iter()
            .map(|status| {
                (
                    status.as_str().into(),
                    status.as_str().into(),
                    issues
                        .iter()
                        .filter(|issue| issue.status == status)
                        .cloned()
                        .collect(),
                )
            })
            .collect(),
        "priority" => PRIORITIES
            .into_iter()
            .map(|priority| {
                (
                    priority.as_str().into(),
                    priority.as_str().into(),
                    issues
                        .iter()
                        .filter(|issue| issue.priority == priority)
                        .cloned()
                        .collect(),
                )
            })
            .collect(),
        "module" => collection
            .modules
            .iter()
            .map(|module| {
                (
                    module.id.to_string(),
                    module.name.clone(),
                    issues
                        .iter()
                        .filter(|issue| issue.module_id == Some(module.id))
                        .cloned()
                        .collect(),
                )
            })
            .chain(std::iter::once((
                "none".into(),
                "No module".into(),
                issues
                    .iter()
                    .filter(|issue| issue.module_id.is_none())
                    .cloned()
                    .collect(),
            )))
            .collect(),
        _ => Vec::new(),
    }
}

fn groups(
    collection: &Collection,
    issues: &[Issue],
    state: &ViewState,
    searching: bool,
) -> Option<Vec<Group>> {
    if searching
        || state.issue_sub_tab == "recent"
        || state.group_by == "none"
        || state.group_by == "status"
            && !state.filter_status.is_empty()
            && state.filter_status != "@unresolved"
    {
        return None;
    }
    Some(
        buckets(collection, issues, &state.group_by)
            .into_iter()
            .filter(|(_, _, issues)| !issues.is_empty())
            .map(|(key, label, issues)| {
                let collapsed = state
                    .collapsed_groups
                    .contains(&format!("{}:{key}", state.group_by));
                Group {
                    key,
                    label,
                    kind: state.group_by.clone(),
                    issues,
                    collapsed,
                }
            })
            .collect(),
    )
}

fn lanes(collection: &Collection, issues: &[Issue], state: &ViewState) -> Option<Vec<Lane>> {
    if state.lane_by == "none" {
        return None;
    }
    Some(
        buckets(collection, issues, &state.lane_by)
            .into_iter()
            .map(|(key, label, issues)| {
                let collapsed = state.collapsed_lanes.contains(&key);
                Lane {
                    key,
                    label,
                    kind: state.lane_by.clone(),
                    issues,
                    collapsed,
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod assignee_tests {
    use super::assignee_filter_matches;
    use crate::db::queries::assignees::{Assignment, IssueAssignee};

    #[test]
    fn issue_assignee_filter_matches_unassigned_human_me_and_username() {
        let assignment = Assignment {
            needs_human: true,
            assignees: vec![IssueAssignee {
                user_id: 17,
                username: "Alice".into(),
                display_name: Some("Alice Example".into()),
            }],
        };
        assert!(assignee_filter_matches(None, 17, "none"));
        assert!(!assignee_filter_matches(Some(&assignment), 17, "none"));
        assert!(assignee_filter_matches(Some(&assignment), 17, "human"));
        assert!(assignee_filter_matches(Some(&assignment), 17, "me"));
        assert!(assignee_filter_matches(Some(&assignment), 18, "@alice"));
        assert!(!assignee_filter_matches(Some(&assignment), 18, "@bob"));
        assert!(assignee_filter_matches(None, 17, ""));
    }
}
