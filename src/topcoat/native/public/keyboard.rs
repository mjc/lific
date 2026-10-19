//! Keyboard navigation ownership for the published issue collection.

use super::super::super::runtime::signal_vec::VecPositionExt;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::Attributes,
};

const ISSUE_ROWS: &str = "li[data-native-issue-row]";
const OPEN_ISSUE: &str = "li[data-native-public-focused='true'] a";
const OVERLAYS: &str = "[role='dialog'],[role='menu'],[data-native-overlay],[data-native-public-mobile],[data-native-markdown-preview]";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    List,
    Board,
}

impl Layout {
    fn has_rows(self) -> bool {
        self == Self::List
    }
}

pub(super) fn owner(cx: &Cx, focused_id: Signal<String>, layout: Layout) -> Attributes {
    let browser = super::super::browser::bindings();
    let last_movement = signal(cx, || -150.0_f64);
    let down_keys = vec!["j", "J", "ArrowDown"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let up_keys = vec!["k", "K", "ArrowUp"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let has_rows = layout.has_rows();
    let mount = expr!(|_event: Event| {
        if !browser.is_disposed() {
            let visible_ids = browser.visible_attribute_values(
                ISSUE_ROWS.to_owned(),
                "data-native-issue-row".to_owned(),
            );
            if has_rows {
                let current = focused_id.get();
                if !current.is_empty() {
                    if visible_ids.position(current).is_none() {
                        focused_id.set("".to_owned());
                    }
                }
            }
            browser.window_listener("keydown".to_owned(), |event: Event| {
                if !browser.is_disposed() {
                    if !browser.is_typing_context() {
                        if !browser.has_visible_match(OVERLAYS.to_owned()) {
                            if has_rows {
                                let visible_ids = browser.visible_attribute_values(
                                    ISSUE_ROWS.to_owned(),
                                    "data-native-issue-row".to_owned(),
                                );
                                let current = focused_id.get();
                                if !current.is_empty() {
                                    if visible_ids.position(current).is_none() {
                                        focused_id.set("".to_owned());
                                    }
                                }
                            }
                            let key = event.key.clone();
                            if key == "/" {
                                event.prevent_default();
                                browser.click_selector(
                                    "[data-native-search-open='issues']".to_owned(),
                                );
                            } else {
                                if has_rows {
                                    let movement_down = down_keys.position(key.clone()).is_some();
                                    let movement_up = up_keys.position(key.clone()).is_some();
                                    let direction = if movement_down {
                                        1_i32
                                    } else if movement_up {
                                        -1_i32
                                    } else {
                                        0_i32
                                    };
                                    let focus_selected = || {
                                        browser.request_frame(|| {
                                            if !browser.is_disposed() {
                                                browser.scroll_selector(
                                                    "li[data-native-public-focused='true']"
                                                        .to_owned(),
                                                );
                                            }
                                        });
                                    };
                                    if direction != 0_i32 {
                                        event.prevent_default();
                                        let next_movement = last_movement.get() + 150.0;
                                        if event.time_stamp >= next_movement {
                                            last_movement.set(event.time_stamp);
                                            let ids = browser.visible_attribute_values(
                                                ISSUE_ROWS.to_owned(),
                                                "data-native-issue-row".to_owned(),
                                            );
                                            if !ids.is_empty() {
                                                let current = focused_id.get();
                                                let position = ids.position(current);
                                                let index = if position.is_some() {
                                                    let previous = position.unwrap();
                                                    if direction > 0_i32 {
                                                        if previous + 1_usize < ids.len() {
                                                            previous + 1_usize
                                                        } else {
                                                            previous
                                                        }
                                                    } else if previous > 0_usize {
                                                        previous - 1_usize
                                                    } else {
                                                        0_usize
                                                    }
                                                } else {
                                                    0_usize
                                                };
                                                focused_id.set(ids.index(index).clone());
                                                browser.call0(focus_selected);
                                            }
                                        }
                                    } else if key == "Home" {
                                        let ids = browser.visible_attribute_values(
                                            ISSUE_ROWS.to_owned(),
                                            "data-native-issue-row".to_owned(),
                                        );
                                        if !ids.is_empty() {
                                            event.prevent_default();
                                            focused_id.set(ids.index(0_usize).clone());
                                            browser.call0(focus_selected);
                                        }
                                    } else if key == "End" {
                                        let ids = browser.visible_attribute_values(
                                            ISSUE_ROWS.to_owned(),
                                            "data-native-issue-row".to_owned(),
                                        );
                                        if !ids.is_empty() {
                                            event.prevent_default();
                                            focused_id.set(ids.index(ids.len() - 1_usize).clone());
                                            browser.call0(focus_selected);
                                        }
                                    } else if key == "Enter" {
                                        let ids = browser.visible_attribute_values(
                                            ISSUE_ROWS.to_owned(),
                                            "data-native-issue-row".to_owned(),
                                        );
                                        if ids.position(focused_id.get()).is_some() {
                                            event.prevent_default();
                                            browser.click_selector(OPEN_ISSUE.to_owned());
                                        }
                                    } else if key == "Escape" {
                                        if !focused_id.get().is_empty() {
                                            focused_id.set("".to_owned());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
    attributes
}
