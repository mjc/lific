//! Pure derivation from the original Home's authorized active/todo read results.
//! Read scope, the 200-row limit per status, and authorization belong upstream.
//! Pinned pages retain their separately documented original query discrepancy.

use crate::db::models::{Issue, Priority, Project, Status};

#[derive(Debug)]
pub(crate) struct HomeIssueGroup<'a> {
    pub(crate) project: &'a Project,
    pub(crate) visible: Vec<&'a Issue>,
    pub(crate) total: usize,
}

#[derive(Debug)]
pub(crate) struct HomeModel<'a> {
    pub(crate) active_issue_count: usize,
    pub(crate) issue_groups: Vec<HomeIssueGroup<'a>>,
    pub(crate) digest_project_ids: Vec<i64>,
    pub(crate) quick_issue_project: Option<&'a Project>,
}

pub(crate) fn derive_home<'a>(projects: &'a [Project], issues: &'a [Issue]) -> HomeModel<'a> {
    // Keep the original Map's first-encounter order before stable sorting.
    let mut by_project: Vec<(i64, Vec<&Issue>)> = Vec::new();
    for issue in issues {
        if let Some((_, group)) = by_project
            .iter_mut()
            .find(|(project_id, _)| *project_id == issue.project_id)
        {
            group.push(issue);
        } else {
            by_project.push((issue.project_id, vec![issue]));
        }
    }

    let mut recent_projects: Vec<_> = by_project
        .iter()
        .filter_map(|(project_id, group)| {
            group
                .iter()
                .map(|issue| issue.updated_at.as_str())
                .max()
                .map(|updated_at| (*project_id, updated_at))
        })
        .collect();
    if recent_projects.is_empty() {
        recent_projects.extend(
            projects
                .iter()
                .map(|project| (project.id, project.updated_at.as_str())),
        );
    }
    recent_projects.sort_by(|a, b| b.1.cmp(a.1));
    let digest_project_ids: Vec<_> = recent_projects
        .into_iter()
        .take(3)
        .map(|(project_id, _)| project_id)
        .collect();
    let quick_issue_project = digest_project_ids
        .first()
        .and_then(|project_id| projects.iter().find(|project| project.id == *project_id))
        .or_else(|| projects.first());

    let mut issue_groups = Vec::new();
    for (project_id, mut group) in by_project {
        let Some(project) = projects.iter().find(|project| project.id == project_id) else {
            continue;
        };
        group.sort_by(|a, b| {
            status_rank(a.status)
                .cmp(&status_rank(b.status))
                .then_with(|| priority_rank(a.priority).cmp(&priority_rank(b.priority)))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        let total = group.len();
        group.truncate(6);
        issue_groups.push(HomeIssueGroup {
            project,
            visible: group,
            total,
        });
    }
    issue_groups.sort_by(|a, b| {
        b.total.cmp(&a.total).then_with(|| {
            // Original Home uses browser localeCompare. Rust string ordering
            // preserves the ASCII fixtures; locale-sensitive case/accent
            // collation still needs an explicit parity decision.
            a.project.name.cmp(&b.project.name)
        })
    });

    HomeModel {
        active_issue_count: issues.len(),
        issue_groups,
        digest_project_ids,
        quick_issue_project,
    }
}

fn status_rank(status: Status) -> u8 {
    match status {
        Status::Active => 0,
        Status::Todo => 1,
        Status::Backlog | Status::Done | Status::Cancelled => 9,
    }
}

fn priority_rank(priority: Priority) -> u8 {
    match priority {
        Priority::Urgent => 0,
        Priority::High => 1,
        Priority::Medium => 2,
        Priority::Low => 3,
        Priority::None => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: i64, name: &str, updated_at: &str) -> Project {
        Project {
            id,
            name: name.into(),
            identifier: format!("P{id}"),
            description: String::new(),
            emoji: None,
            lead_user_id: None,
            sort_order: 0,
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
            is_public: false,
        }
    }

    fn issue(id: i64, project_id: i64, status: Status, priority: Priority, time: &str) -> Issue {
        Issue {
            id,
            project_id,
            sequence: id,
            identifier: format!("P{project_id}-{id}"),
            title: format!("Issue {id}"),
            description: String::new(),
            status,
            priority,
            module_id: None,
            sort_order: 0.0,
            start_date: None,
            target_date: None,
            created_at: time.into(),
            updated_at: time.into(),
            seq: id,
            source: None,
            labels: Vec::new(),
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            relates_to: Vec::new(),
            duplicates: Vec::new(),
            duplicated_by: Vec::new(),
            waits: Vec::new(),
        }
    }

    fn visible_ids(group: &HomeIssueGroup<'_>) -> Vec<i64> {
        group.visible.iter().map(|issue| issue.id).collect()
    }

    #[test]
    fn issue_order_uses_status_then_priority_then_latest_update() {
        let projects = [project(1, "Alpha", "2026-10-01")];
        let issues = [
            issue(1, 1, Status::Todo, Priority::Urgent, "2026-10-09"),
            issue(2, 1, Status::Active, Priority::None, "2026-10-01"),
            issue(3, 1, Status::Active, Priority::High, "2026-10-03"),
            issue(4, 1, Status::Active, Priority::High, "2026-10-02"),
            issue(5, 1, Status::Active, Priority::Urgent, "2026-10-01"),
            issue(6, 1, Status::Todo, Priority::Urgent, "2026-10-08"),
        ];
        let model = derive_home(&projects, &issues);
        assert_eq!(visible_ids(&model.issue_groups[0]), [5, 3, 4, 2, 1, 6]);
        assert_eq!(
            issues.iter().map(|issue| issue.id).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn six_row_cap_preserves_group_total_and_full_priority_order() {
        let projects = [project(1, "Alpha", "2026-10-01")];
        let issues = [
            issue(7, 1, Status::Todo, Priority::None, "2026-10-01"),
            issue(5, 1, Status::Active, Priority::None, "2026-10-01"),
            issue(4, 1, Status::Active, Priority::Low, "2026-10-01"),
            issue(3, 1, Status::Active, Priority::Medium, "2026-10-01"),
            issue(2, 1, Status::Active, Priority::High, "2026-10-01"),
            issue(1, 1, Status::Active, Priority::Urgent, "2026-10-01"),
            issue(6, 1, Status::Todo, Priority::Urgent, "2026-10-01"),
        ];
        let model = derive_home(&projects, &issues);
        assert_eq!(model.active_issue_count, 7);
        assert_eq!(model.issue_groups[0].total, 7);
        assert_eq!(visible_ids(&model.issue_groups[0]), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn groups_order_by_count_then_name_and_skip_projects_missing_from_the_list() {
        let projects = [
            project(1, "Zulu", "2026-10-01"),
            project(2, "Alpha", "2026-10-01"),
            project(3, "Many", "2026-10-01"),
        ];
        let issues = [
            issue(1, 1, Status::Active, Priority::None, "2026-10-01"),
            issue(2, 2, Status::Active, Priority::None, "2026-10-01"),
            issue(3, 3, Status::Active, Priority::None, "2026-10-01"),
            issue(4, 3, Status::Todo, Priority::None, "2026-10-01"),
            issue(5, 99, Status::Todo, Priority::None, "2026-10-01"),
        ];
        let model = derive_home(&projects, &issues);
        assert_eq!(model.active_issue_count, 5);
        assert_eq!(
            model
                .issue_groups
                .iter()
                .map(|group| group.project.id)
                .collect::<Vec<_>>(),
            [3, 2, 1]
        );
    }

    #[test]
    fn digest_uses_each_projects_latest_issue_and_keeps_only_three() {
        let projects = [
            project(1, "One", "2026-10-30"),
            project(2, "Two", "2026-10-30"),
            project(3, "Three", "2026-10-30"),
            project(4, "Four", "2026-10-30"),
        ];
        let issues = [
            issue(1, 1, Status::Active, Priority::None, "2026-10-01"),
            issue(2, 2, Status::Todo, Priority::None, "2026-10-04"),
            issue(3, 3, Status::Active, Priority::None, "2026-10-03"),
            issue(4, 4, Status::Todo, Priority::None, "2026-10-02"),
            issue(5, 1, Status::Todo, Priority::None, "2026-10-05"),
        ];
        let model = derive_home(&projects, &issues);
        assert_eq!(model.digest_project_ids, [1, 2, 3]);
        assert_eq!(model.quick_issue_project.unwrap().id, 1);
    }

    #[test]
    fn no_issues_uses_three_latest_projects_and_preserves_tie_order() {
        let projects = [
            project(1, "First", "2026-10-01"),
            project(2, "Second", "2026-10-05"),
            project(3, "Third", "2026-10-05"),
            project(4, "Fourth", "2026-10-03"),
        ];
        let model = derive_home(&projects, &[]);
        assert_eq!(model.digest_project_ids, [2, 3, 4]);
        assert_eq!(model.quick_issue_project.unwrap().id, 2);
        assert_eq!(model.active_issue_count, 0);
        assert!(model.issue_groups.is_empty());
    }

    #[test]
    fn equal_issue_times_preserve_encounter_order_and_missing_digest_target_falls_back() {
        let projects = [
            project(1, "First", "2026-10-01"),
            project(2, "Second", "2026-10-01"),
        ];
        let issues = [
            issue(1, 99, Status::Active, Priority::None, "2026-10-01"),
            issue(2, 2, Status::Active, Priority::None, "2026-10-01"),
            issue(3, 1, Status::Active, Priority::None, "2026-10-01"),
            issue(4, 2, Status::Active, Priority::None, "2026-10-01"),
        ];
        let model = derive_home(&projects, &issues);
        assert_eq!(model.digest_project_ids, [99, 2, 1]);
        assert_eq!(model.quick_issue_project.unwrap().id, 1);
        let second = model
            .issue_groups
            .iter()
            .find(|group| group.project.id == 2)
            .unwrap();
        assert_eq!(visible_ids(second), [2, 4]);
    }

    #[test]
    fn empty_home_hides_the_new_issue_target() {
        let model = derive_home(&[], &[]);
        assert_eq!(model.active_issue_count, 0);
        assert!(model.issue_groups.is_empty());
        assert!(model.digest_project_ids.is_empty());
        assert!(model.quick_issue_project.is_none());
    }
}
