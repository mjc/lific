//! Browser persistence for published issue-list preferences.

use super::super::browser;
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Signal, expr},
    view::Attributes,
};

pub(super) const ISSUE_DEFAULTS: &str = r#"{"filterStatus":"","filterPriority":"","filterLabel":"","filterModule":"","searchQuery":"","sortField":"priority","sortDir":"asc","groupBy":"status"}"#;

pub(super) fn issue_storage_key(project: &str) -> String {
    format!("lific:public:list:state:{project}")
}

pub(super) fn issue_tab_storage_key(project_id: i64) -> String {
    format!("lific:public:subtab:issues:{project_id}")
}

pub(super) fn page_tab_storage_key(project_id: i64) -> String {
    format!("lific:public:subtab:pages:{project_id}")
}

pub(super) struct IssueSignals {
    pub wire: Signal<String>,
    pub hydrated: Signal<bool>,
    pub query: Signal<String>,
    pub status: Signal<String>,
    pub priority: Signal<String>,
    pub label: Signal<String>,
    pub module: Signal<String>,
    pub sort_field: Signal<String>,
    pub sort_dir: Signal<String>,
    pub group_by: Signal<String>,
    pub issue_sub_tab: Signal<String>,
}

pub(super) fn issue_mount(
    cx: &Cx,
    project: &str,
    project_id: i64,
    layout: &str,
    signals: IssueSignals,
) -> Attributes {
    let browser = browser::bindings();
    let IssueSignals {
        wire,
        hydrated,
        query,
        status,
        priority,
        label,
        module,
        sort_field,
        sort_dir,
        group_by,
        issue_sub_tab,
    } = signals;
    let key = issue_storage_key(project);
    let tab_key = issue_tab_storage_key(project_id);
    let layout_key = format!("lific:public:list:layout:{project}");
    let defaults = ISSUE_DEFAULTS.to_owned();
    let layout = layout.to_owned();
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            if !hydrated.get() {
                let stored = browser.json_fields(browser.stored(key.clone()), defaults.clone());
                wire.set(stored.clone());
                query.set(browser.json_string(
                    stored.clone(),
                    "searchQuery".to_owned(),
                    "".to_owned(),
                ));
                status.set(browser.json_string(
                    stored.clone(),
                    "filterStatus".to_owned(),
                    "".to_owned(),
                ));
                priority.set(browser.json_string(
                    stored.clone(),
                    "filterPriority".to_owned(),
                    "".to_owned(),
                ));
                label.set(browser.json_string(
                    stored.clone(),
                    "filterLabel".to_owned(),
                    "".to_owned(),
                ));
                module.set(browser.json_string(
                    stored.clone(),
                    "filterModule".to_owned(),
                    "".to_owned(),
                ));
                sort_field.set(browser.json_string(
                    stored.clone(),
                    "sortField".to_owned(),
                    "priority".to_owned(),
                ));
                sort_dir.set(browser.json_string(
                    stored.clone(),
                    "sortDir".to_owned(),
                    "asc".to_owned(),
                ));
                group_by.set(browser.json_string(
                    stored,
                    "groupBy".to_owned(),
                    "status".to_owned(),
                ));
                let tab = browser.stored(tab_key.clone());
                if tab == "recent" {
                    issue_sub_tab.set(tab);
                } else if tab == "open" {
                    issue_sub_tab.set(tab);
                } else if tab == "closed" {
                    issue_sub_tab.set(tab);
                }
                hydrated.set(true);
            }
            browser.store(layout_key.clone(), layout.clone());
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, "data-native-public-preferences", "issues".to_owned());
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn pages_mount(cx: &Cx, project_id: i64, tab: Signal<String>) -> Attributes {
    let browser = browser::bindings();
    let key = page_tab_storage_key(project_id);
    let handler = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let saved = browser.stored(key.clone());
            if saved == "browse" {
                tab.set(saved);
            } else if saved == "recent" {
                tab.set(saved);
            } else if saved == "drafts" {
                tab.set(saved);
            } else if saved == "archived" {
                tab.set(saved);
            }
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, "data-native-public-preferences", "pages".to_owned());
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn save_page_tab(
    tab: Signal<String>,
    key: String,
    selected: &'static str,
) -> Expr<impl FnOnce(Event)> {
    let browser = browser::bindings();
    expr!(|_event: Event| {
        if !browser.is_disposed() {
            tab.set(selected.to_owned());
            browser.store(key.clone(), selected.to_owned());
        }
    })
}

pub(super) fn persist_input(
    state: Signal<String>,
    wire: Signal<String>,
    key: String,
    field: &str,
) -> Expr<impl FnOnce(Event)> {
    let browser = browser::bindings();
    let field = field.to_owned();
    expr!(|event: Event| {
        if !browser.is_disposed() {
            let value = event.target.value.to_owned();
            state.set(value.clone());
            let next = browser.json_set_string(wire.get(), field.clone(), value);
            wire.set(next.clone());
            browser.store(key.clone(), next);
        }
    })
}
