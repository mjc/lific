use super::{
    data::Collection,
    model::{self, ViewState},
};
use crate::db::models::{Issue, Module, Priority, Project, Status};

fn issue(id: i64, status: Status, priority: Priority) -> Issue {
    Issue {
        id,
        project_id: 1,
        sequence: id,
        identifier: format!("ACC-{id}"),
        title: format!("Work {id}"),
        description: String::new(),
        status,
        priority,
        module_id: None,
        sort_order: id as f64,
        start_date: None,
        target_date: None,
        created_at: format!("2026-01-{:02} 00:00:00", id % 28 + 1),
        updated_at: format!("2026-02-{:02} 00:00:00", id % 28 + 1),
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

fn module(id: i64, name: &str) -> Module {
    Module {
        id,
        project_id: 1,
        name: name.into(),
        description: String::new(),
        status: "active".into(),
        emoji: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn collection(issues: Vec<Issue>) -> Collection {
    Collection {
        project: Project {
            id: 1,
            name: "Project".into(),
            identifier: "ACC".into(),
            description: String::new(),
            emoji: None,
            lead_user_id: None,
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
            is_public: false,
        },
        modules: vec![module(7, "Design"), module(8, "Empty")],
        labels: Vec::new(),
        issues,
    }
}

fn ids(issues: &[Issue]) -> Vec<i64> {
    issues.iter().map(|issue| issue.id).collect()
}

#[test]
fn issue_collection_filters_compose_and_counts_remain_project_wide() {
    let mut target = issue(1, Status::Active, Priority::High);
    target.module_id = Some(7);
    target.labels = vec!["Bug".into()];
    let mut other_status = target.clone();
    other_status.id = 2;
    other_status.status = Status::Done;
    let mut other_priority = target.clone();
    other_priority.id = 3;
    other_priority.priority = Priority::Low;
    let mut other_label = target.clone();
    other_label.id = 4;
    other_label.labels = vec!["bug".into()];
    let mut other_module = target.clone();
    other_module.id = 5;
    other_module.module_id = None;
    let collection = collection(vec![
        target,
        other_status,
        other_priority,
        other_label,
        other_module,
    ]);
    let state = ViewState {
        filter_status: "@unresolved".into(),
        filter_priority: "high".into(),
        filter_label: "Bug".into(),
        filter_module: "Design".into(),
        ..Default::default()
    };
    let selected = model::select(&collection, &state, "list");
    assert_eq!(ids(&selected.issues), [1]);
    assert_eq!(selected.count_label, "1 of 5");
    assert_eq!(selected.stats.total, 5);
    assert_eq!(selected.stats.statuses, [0, 0, 4, 1, 0]);
    assert_eq!(selected.stats.priorities, [0, 4, 0, 1, 0]);
    assert_eq!(selected.stats.by_module.get(&7), Some(&4));
    assert_eq!(selected.stats.no_module, 1);
}

#[test]
fn issue_collection_unknown_stored_module_name_selects_unassigned_like_main() {
    let mut assigned = issue(1, Status::Todo, Priority::None);
    assigned.module_id = Some(7);
    let collection = collection(vec![assigned, issue(2, Status::Todo, Priority::None)]);
    let state = ViewState {
        filter_module: "Deleted module".into(),
        ..Default::default()
    };
    assert_eq!(ids(&model::select(&collection, &state, "list").issues), [2]);
}

#[test]
fn issue_collection_priority_direction_reverses_its_creation_tie_break() {
    let collection = collection(vec![
        issue(1, Status::Todo, Priority::High),
        issue(2, Status::Todo, Priority::High),
        issue(3, Status::Todo, Priority::Urgent),
    ]);
    assert_eq!(
        ids(&model::select(&collection, &ViewState::default(), "list").issues),
        [3, 2, 1]
    );
    let state = ViewState {
        sort_dir: "desc".into(),
        ..Default::default()
    };
    assert_eq!(
        ids(&model::select(&collection, &state, "list").issues),
        [1, 2, 3]
    );
    for (field, expected) in [
        ("age", vec![1, 2, 3]),
        ("number", vec![1, 2, 3]),
        ("updated", vec![1, 2, 3]),
    ] {
        let state = ViewState {
            sort_field: field.into(),
            ..Default::default()
        };
        assert_eq!(
            ids(&model::select(&collection, &state, "list").issues),
            expected
        );
    }
}

#[test]
fn issue_collection_search_uses_main_weights_snippets_and_ecmascript_trim() {
    let mut title = issue(1, Status::Todo, Priority::Low);
    title.title = "needle title".into();
    let mut identifier = issue(2, Status::Todo, Priority::Urgent);
    identifier.identifier = "NEEDLE-2".into();
    let mut body = issue(3, Status::Todo, Priority::Urgent);
    body.description = "needle preview".into();
    let mut label = issue(4, Status::Todo, Priority::Urgent);
    label.labels = vec!["needle".into()];
    let collection = collection(vec![label, body, identifier, title]);
    let state = ViewState {
        search_query: "\u{feff}needle\u{a0}".into(),
        ..Default::default()
    };
    let selected = model::select(&collection, &state, "list");
    assert_eq!(ids(&selected.issues), [1, 2, 3]);
    assert!(selected.searching);
    assert!(selected.groups.is_none());
    assert_eq!(
        selected.snippets.get(&3).map(String::as_str),
        Some("needle preview")
    );
    assert!(!selected.snippets.contains_key(&1));
}

#[test]
fn issue_collection_search_caps_after_filters_before_list_recent_slice() {
    let rows = (1..=75)
        .map(|id| {
            let mut row = issue(
                id,
                if id <= 20 { Status::Done } else { Status::Todo },
                Priority::None,
            );
            row.title = "needle".into();
            row.updated_at = format!("2026-03-01 00:{id:02}:00");
            row
        })
        .collect();
    let collection = collection(rows);
    let state = ViewState {
        filter_status: "todo".into(),
        search_query: "needle".into(),
        ..Default::default()
    };
    let selected = model::select(&collection, &state, "list");
    assert_eq!(selected.issues.len(), 50);
    assert!(selected.show_search_cap);
    assert_eq!(selected.count_label, "50 of 75");
    assert!(
        selected
            .issues
            .iter()
            .all(|row| row.id > 20 && row.id <= 70)
    );
    let state = ViewState {
        issue_sub_tab: "recent".into(),
        ..state
    };
    let selected = model::select(&collection, &state, "list");
    assert_eq!(selected.issues.len(), 20);
    assert_eq!(selected.issues[0].id, 70);
    assert!(!selected.show_search_cap);
    assert!(selected.groups.is_none());
    assert_eq!(model::select(&collection, &state, "board").issues.len(), 50);
}

#[test]
fn issue_collection_subtabs_grouping_and_board_visibility_are_independent() {
    let collection = collection(vec![
        issue(1, Status::Backlog, Priority::None),
        issue(2, Status::Done, Priority::None),
        issue(3, Status::Cancelled, Priority::None),
    ]);
    let state = ViewState {
        issue_sub_tab: "closed".into(),
        ..Default::default()
    };
    let selected = model::select(&collection, &state, "list");
    assert_eq!(selected.issues.len(), 2);
    assert_eq!(selected.groups.as_ref().unwrap().len(), 2);
    assert_eq!(model::select(&collection, &state, "board").issues.len(), 3);
    let state = ViewState {
        filter_status: "done".into(),
        ..Default::default()
    };
    assert!(model::select(&collection, &state, "list").groups.is_none());
    assert_eq!(
        model::select(&collection, &state, "board").visible_statuses,
        [Status::Done]
    );
    let state = ViewState {
        filter_status: "@unresolved".into(),
        ..Default::default()
    };
    assert_eq!(
        model::select(&collection, &state, "list")
            .groups
            .unwrap()
            .len(),
        1
    );
    assert!(
        model::select(&collection, &state, "board")
            .visible_statuses
            .is_empty(),
        "Main's literal board-column predicate does not recognize its unresolved sentinel"
    );
}

#[test]
fn issue_collection_groups_omit_empty_modules_but_lanes_keep_them_and_folds() {
    let mut assigned = issue(1, Status::Todo, Priority::None);
    assigned.module_id = Some(7);
    let collection = collection(vec![assigned, issue(2, Status::Todo, Priority::None)]);
    let state = ViewState {
        group_by: "module".into(),
        lane_by: "module".into(),
        collapsed_groups: vec!["module:7".into()],
        collapsed_lanes: vec!["8".into()],
        hidden_statuses: vec!["cancelled".into()],
        collapsed_columns: vec!["todo".into()],
        ..Default::default()
    };
    let selected = model::select(&collection, &state, "list");
    let groups = selected.groups.unwrap();
    assert_eq!(
        groups
            .iter()
            .map(|group| group.key.as_str())
            .collect::<Vec<_>>(),
        ["7", "none"]
    );
    assert!(groups[0].collapsed);
    assert_eq!(groups[1].label, "No module");
    let selected = model::select(&collection, &state, "board");
    let lanes = selected.lanes.unwrap();
    assert_eq!(lanes.len(), 3);
    assert_eq!(lanes[1].label, "Empty");
    assert!(lanes[1].issues.is_empty());
    assert!(lanes[1].collapsed);
    assert_eq!(selected.visible_statuses.len(), 4);
    assert_eq!(selected.collapsed_columns, ["todo"]);
    assert_eq!(
        selected.count_label, "2",
        "folds do not narrow the collection count"
    );
}

#[test]
fn issue_collection_filtered_empty_is_distinct_from_a_blank_project() {
    let empty = collection(Vec::new());
    assert!(!model::select(&empty, &ViewState::default(), "list").empty_filtered);
    let state = ViewState {
        search_query: "missing".into(),
        ..Default::default()
    };
    assert!(model::select(&empty, &state, "list").empty_filtered);
    let state = ViewState {
        issue_sub_tab: "open".into(),
        ..Default::default()
    };
    assert!(model::select(&empty, &state, "list").empty_filtered);
    assert!(!model::select(&empty, &state, "board").empty_filtered);
}
