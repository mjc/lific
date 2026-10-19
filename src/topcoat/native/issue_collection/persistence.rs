//! Main's independent project-scoped storage slices.
use super::model::ViewState;

pub(super) const DEFAULTS: &str = r#"{"filterStatus":"","filterPriority":"","filterLabel":"","filterModule":"","filterAssignee":"","searchQuery":"","sortField":"priority","sortDir":"asc","groupBy":"status","density":"compact"}"#;

pub(super) fn state(wire: &str, tab: String, lane: String, slices: &[String; 4]) -> ViewState {
    let mut state: ViewState = serde_json::from_str(wire).unwrap_or_default();
    state.issue_sub_tab = tab;
    state.lane_by = lane;
    state.collapsed_groups = serde_json::from_str(&slices[0]).unwrap_or_default();
    state.hidden_statuses = serde_json::from_str(&slices[1]).unwrap_or_default();
    state.collapsed_lanes = serde_json::from_str(&slices[2]).unwrap_or_default();
    state.collapsed_columns = serde_json::from_str(&slices[3]).unwrap_or_default();
    state
}
