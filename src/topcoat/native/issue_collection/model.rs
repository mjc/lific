//! Shared, server-side issue list and board selection.
use std::collections::HashMap;

use crate::db::models::{Issue, Status};

use super::data::Collection;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct ViewState {
    pub filter_status: String,
    pub filter_priority: String,
    pub filter_label: String,
    pub filter_module: String,
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

#[derive(Debug, Default)]
pub(super) struct Stats {
    pub total: usize,
    pub statuses: [usize; 5],
    pub priorities: [usize; 5],
    pub by_module: HashMap<i64, usize>,
    pub no_module: usize,
}

#[derive(Debug)]
pub(super) struct Group {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub issues: Vec<Issue>,
    pub collapsed: bool,
}

#[derive(Debug)]
pub(super) struct Lane {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub issues: Vec<Issue>,
    pub collapsed: bool,
}

#[derive(Debug)]
pub(super) struct Selection {
    pub issues: Vec<Issue>,
    pub groups: Option<Vec<Group>>,
    pub lanes: Option<Vec<Lane>>,
    pub visible_statuses: Vec<Status>,
    pub count_label: String,
    pub stats: Stats,
    pub searching: bool,
    pub show_search_cap: bool,
    pub empty_filtered: bool,
    pub layout: String,
    pub density: String,
    pub collapsed_columns: Vec<String>,
    pub snippets: HashMap<i64, String>,
}

pub(super) fn select(_collection: &Collection, _state: &ViewState, _layout: &str) -> Selection {
    unimplemented!("native issue collection selection is not implemented")
}
