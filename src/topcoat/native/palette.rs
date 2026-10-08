//! Native command palette: authorized projections own their result and selection model.

use super::super::runtime::{connected, signal_vec::VecPositionExt};
use super::super::shell::ParsedRoute;
use super::{icons::UiIcon, session::native_home_session};
use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Js, Signal, SignalSurrogate, StringSurrogate,
        Surrogated, VecSurrogate, expr, record, shard,
    },
    view::{Attributes, BoxView, View, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct PaletteState {
    pub(super) open: Signal<bool>,
    pub(super) query: Signal<String>,
    pub(super) searched: Signal<String>,
    pub(super) revision: Signal<usize>,
    pub(super) authorized: Signal<usize>,
    pub(super) rendered: Signal<usize>,
    pub(super) selected: Signal<usize>,
    pub(super) selected_href: Signal<String>,
    pub(super) cursor_moved: Signal<bool>,
    pub(super) count: Signal<usize>,
    pub(super) pending_enter: Signal<bool>,
    pub(super) pending_new_tab: Signal<bool>,
    pub(super) waiting: Signal<bool>,
    pub(super) error: Signal<String>,
    pub(super) pending_focus: Signal<bool>,
    pub(super) account_id: i64,
    pub(super) is_admin: bool,
}

pub(super) type PaletteSignals = (
    Signal<usize>,
    Signal<usize>,
    Signal<String>,
    Signal<bool>,
    Signal<usize>,
    Signal<usize>,
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
    Signal<String>,
    (Signal<String>, Signal<bool>),
);

/// The mobile component receives this typed callback through the shared factory.
pub(super) struct OpenPalette;
impl OpenPalette {
    pub(super) fn call(&self, _opener: StringSurrogate) {}
}

#[record]
#[derive(Clone, Debug)]
pub(super) struct Projection {
    revision: usize,
    destinations: Vec<String>,
}

type ProjectionSurrogate = <Projection as Surrogated>::Surrogate;

type ProjectionSignals<'a> = (
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    (&'a SignalSurrogate<String>, &'a SignalSurrogate<bool>),
);

/// Each input captures the same authorized Rust projection as its visible rows.
/// Ordinary input listeners are installed during hydration, before queued mounts.
pub(super) fn projection_factory() -> Js {
    let browser = super::browser::bindings();
    expr!(|event: Event,
           state: ProjectionSignals<'_>,
           projection: ProjectionSurrogate,
           allowed: BoolSurrogate,
           mode: StringSurrogate| {
        let revision = state.0;
        let selected = state.1;
        let selected_href = state.2;
        let cursor = state.3;
        let count = state.4;
        let rendered = state.5;
        let pending_enter = state.6;
        let pending_tab = state.7;
        let waiting = state.8;
        let open = state.9;
        let pending_focus = state.11.1;
        let current = if browser.is_disposed() {
            false
        } else {
            if open.get() {
                revision.get() == projection.revision
            } else {
                false
            }
        };
        if mode == "key" {
            if !browser.is_disposed() {
                if open.get() {
                    if waiting.get() {
                        if event.key.clone() == "Enter" {
                            event.prevent_default();
                            pending_enter.set(true);
                            pending_tab.set(if event.meta_key { true } else { event.ctrl_key });
                        } else {
                            if event.key.clone() == "ArrowDown" {
                                event.prevent_default();
                            } else {
                                if event.key.clone() == "ArrowUp" {
                                    event.prevent_default();
                                }
                            }
                        }
                    }
                }
            }
        }
        if current {
            if mode == "focus" {
                if pending_focus.get() {
                    if browser.focus_id("native-home-palette-query".to_owned()) {
                        pending_focus.set(false);
                    }
                }
            } else {
                if allowed {
                    let destinations = projection.destinations;
                    let total = destinations.len();
                    count.set(total);
                    if cursor.get() {
                        let retained = destinations.position(selected_href.get());
                        if retained.is_some() {
                            selected.set(retained.unwrap());
                        }
                    } else {
                        selected.set(0usize);
                    }
                    if total == 0usize {
                        selected.set(0usize);
                        selected_href.set("".to_owned());
                    } else {
                        if selected.get() >= total {
                            selected.set(total - 1usize);
                        }
                        selected_href.set(destinations.get(selected.get()).unwrap().to_owned());
                    }
                    rendered.set(projection.revision);
                    waiting.set(false);
                    let key = if mode == "key" {
                        event.key.clone()
                    } else {
                        "".to_owned()
                    };
                    if key == "ArrowDown" {
                        event.prevent_default();
                        cursor.set(true);
                        if selected.get() + 1usize < total {
                            selected.increment();
                        }
                    } else {
                        if key == "ArrowUp" {
                            event.prevent_default();
                            cursor.set(true);
                            if selected.get() > 0usize {
                                selected.decrement();
                            }
                        }
                    }
                    let arrow = if key == "ArrowDown" {
                        true
                    } else {
                        key == "ArrowUp"
                    };
                    if arrow {
                        if total > 0usize {
                            selected_href.set(destinations.get(selected.get()).unwrap().to_owned());
                            let _index = selected.get();
                            browser.scroll_selector(raw!("cx.hydrate('.native-home-palette-results a[data-palette-index=\"'+${_index}.toString()+'\"]')", String::new()));
                        }
                    }
                    let enter = if key == "Enter" {
                        true
                    } else {
                        if mode == "mount" {
                            pending_enter.get()
                        } else {
                            false
                        }
                    };
                    if enter {
                        let new_tab = if mode == "mount" {
                            pending_tab.get()
                        } else {
                            if event.meta_key { true } else { event.ctrl_key }
                        };
                        if mode == "key" {
                            event.prevent_default();
                        }
                        pending_enter.set(false);
                        pending_tab.set(false);
                        let destination = selected_href.get();
                        if !destination.is_empty() {
                            open.set(false);
                            revision.increment();
                            pending_focus.set(false);
                            if new_tab {
                                browser.open_tab(destination);
                            } else {
                                browser.navigate(destination);
                            }
                        }
                    }
                } else {
                    if mode == "key" {
                        if event.key.clone() == "Enter" {
                            event.prevent_default();
                            pending_enter.set(true);
                            pending_tab.set(if event.meta_key { true } else { event.ctrl_key });
                        }
                    }
                }
            }
        }
    })
    .into_evaluated_and_js()
    .1
}

type PaletteHandlerSignals<'a> = (
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
);

type PaletteStatusSignals<'a> = (
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
);

type PaletteRequest<'a> = (
    &'a VecSurrogate<String>,
    &'a StringSurrogate,
    I64Surrogate,
    BoolSurrogate,
);

pub(super) fn handler_factory() -> Js {
    let browser = super::browser::bindings();
    expr!(|_mount: Event,
           chrome: super::mobile_navigation::ChromeHandlerSignals<'_>,
           palette: PaletteHandlerSignals<'_>,
           status: PaletteStatusSignals<'_>,
           request: PaletteRequest<'_>| {
        let theme_menu = chrome.2;
        let mobile_open = chrome.3;
        let sidebar_menu = chrome.11;
        let palette_open = palette.0;
        let palette_query = palette.1;
        let palette_searched = palette.2;
        let palette_revision = palette.3;
        let palette_authorized = palette.4;
        let palette_selected = palette.6;
        let palette_selected_href = palette.7;
        let palette_cursor_moved = palette.8;
        let palette_count = palette.9;
        let palette_pending_enter = palette.10;
        let palette_pending_new_tab = palette.11;
        let pending_focus = palette.12;
        let palette_waiting = status.0;
        let palette_error = status.1;
        let palette_return_focus = status.2;
        let _login = request.1;
        let account_id = request.2;
        let is_admin = request.3;
        let _dispose_palette = || {
            palette_revision.increment();
        };
        let _start_query = || {
            palette_revision.increment();
            let sent_revision = palette_revision.get();
            let value = palette_query.get();
            palette_count.set(0usize);
            palette_selected.set(0usize);
            palette_selected_href.set("".to_owned());
            palette_cursor_moved.set(false);
            palette_pending_enter.set(false);
            palette_pending_new_tab.set(false);
            palette_error.set("".to_owned());
            palette_waiting.set(true);
            let _failed = || {
                if !browser.is_disposed() {
                    if palette_revision.get() == sent_revision {
                        palette_error.set("Unable to search. Try again.".to_owned());
                        palette_waiting.set(false);
                        palette_pending_enter.set(false);
                        palette_pending_new_tab.set(false);
                    }
                }
            };
            let _request = async || {
                if !browser.is_disposed() {
                    let current = native_home_session().await;
                    if !browser.is_disposed() {
                        if palette_revision.get() == sent_revision {
                            if current.is_none() {
                                raw!("window.location.assign(${_login}.toString())", ());
                            } else {
                                let current_account = current.unwrap();
                                let changed = if current_account.0 != account_id {
                                    true
                                } else {
                                    current_account.1 != is_admin
                                };
                                if changed {
                                    raw!("window.location.reload()", ());
                                } else {
                                    palette_searched.set(value);
                                    palette_authorized.set(sent_revision);
                                }
                            }
                        }
                    }
                }
            };
            raw!(
                "Promise.resolve().then(() => ${_request}()).catch(() => ${_failed}());",
                ()
            );
        };
        let _open_palette = |opener: StringSurrogate| {
            palette_return_focus.set(opener);
            pending_focus.set(true);
            palette_query.set("".to_owned());
            palette_open.set(true);
            browser.call0(_start_query);
            let opened_revision = palette_revision.get();
            let _focus_palette = || {
                if !browser.is_disposed() {
                    if palette_open.get() {
                        if palette_revision.get() == opened_revision {
                            if browser.focus_id("native-home-palette-query".to_owned()) {
                                pending_focus.set(false);
                            }
                        }
                    }
                }
            };
            browser.microtask(_focus_palette);
        };
        let _close_palette = || {
            pending_focus.set(false);
            palette_revision.increment();
            palette_open.set(false);
            palette_pending_enter.set(false);
            palette_pending_new_tab.set(false);
            palette_waiting.set(false);
        };
        let _palette_input = |_event: Event| {
            let id = _event.target.id;
            if id == "native-home-palette-query" {
                palette_query.set(_event.target.value);
                browser.call0(_start_query);
            }
        };

        let _palette_opener = |_event: Event| {
            let opener = raw!(
                "cx.hydrate(${_event}.inner.target.closest('#native-home-palette-open,#native-home-quick-jump,#native-home-palette-close')?.id || '')",
                String::new()
            );
            if opener == "native-home-palette-close" {
                browser.call0(_close_palette);
            } else {
                if !opener.is_empty() {
                    browser.call1(_open_palette, opener);
                }
            }
        };

        let keyboard = |event: Event| {
            if sidebar_menu.get().is_empty() {
                if !theme_menu.get() {
                    if !mobile_open.get() {
                        if event.key.clone() == "Escape" {
                            if palette_open.get() {
                                browser.call0(_close_palette);
                                event.prevent_default();
                                event.stop_propagation();
                                let opener = palette_return_focus.get();
                                let focus = || {
                                    browser.focus_id(opener.clone());
                                };
                                browser.microtask(focus);
                            }
                        }
                    }
                }
            }
        };
        browser.window_listener("keydown".to_owned(), keyboard);
        browser.click_capture(_palette_opener);
        browser.window_listener("input".to_owned(), _palette_input);
        browser.on_dispose(_dispose_palette);
        raw!("${_open_palette}", ())
    })
    .into_evaluated_and_js()
    .1
}

pub(super) fn view<'a>(cx: &'a Cx, palette: PaletteState, path: Signal<String>) -> BoxView<'a> {
    let palette_open = palette.open.clone();
    let query = palette.query.clone();
    let searched = palette.searched.clone();
    let revision = palette.revision.clone();
    let authorized = palette.authorized.clone();
    view! {
        cx =>
        <div class="native-home-palette-backdrop" :hidden=$(!palette_open.get())>
            <section
                class="native-home-palette"
                role="dialog"
                aria-modal="true"
                aria-labelledby="native-home-palette-title"
            >
                <header>
                    <h2 id="native-home-palette-title">"Jump to project"</h2>
                    <button
                        id="native-home-palette-close"
                        class="native-home-icon-button"
                        aria-label="Close project search"
                        @click=$(|_event| palette_open.set(false))
                    >
                        (super::icons::ui_icon(cx, UiIcon::Close, 18))
                    </button>
                </header>
                <label for="native-home-palette-query">
                    "Search visible projects and issue references"
                </label>
                native_home_palette_results(
                    query: $(searched.get()),
                    open: $(palette_open.get()),
                    revision: $(revision.get()),
                    authorized: $(authorized.get()),
                    error_signal: palette.error.clone(),
                    state: (
                        palette.revision.clone(),
                        palette.selected.clone(),
                        palette.selected_href.clone(),
                        palette.cursor_moved.clone(),
                        palette.count.clone(),
                        palette.rendered.clone(),
                        palette.pending_enter.clone(),
                        palette.pending_new_tab.clone(),
                        palette.waiting.clone(),
                        palette.open.clone(),
                        path.clone(),
                        (query.clone(), palette.pending_focus.clone()),
                    )
                )
            </section>
        </div>
    }
    .boxed()
}
pub(super) fn matching_projects<'a>(
    projects: &'a [crate::db::models::Project],
    query: &str,
) -> Vec<&'a crate::db::models::Project> {
    let query = query
        .trim()
        .chars()
        .take(128)
        .collect::<String>()
        .to_lowercase();
    projects
        .iter()
        .filter(|project| {
            project.name.to_lowercase().contains(&query)
                || project.identifier.to_lowercase().contains(&query)
        })
        .take(20)
        .collect()
}

#[shard("/__native_home/palette")]
pub(super) async fn native_home_palette_results(
    cx: &Cx,
    query: String,
    open: bool,
    revision: usize,
    authorized: usize,
    state: PaletteSignals,
    error_signal: Signal<String>,
) -> topcoat::Result<impl View> {
    let (
        live_revision,
        selected,
        selected_href,
        cursor_moved,
        count,
        rendered,
        pending_enter,
        pending_new_tab,
        waiting,
        live_open,
        path,
        (live_query, pending_focus),
    ) = state;
    let current_path = path.get_untracked();
    let current_project = ParsedRoute::parse(&current_path).project.map(str::to_owned);
    let connected = connected(cx);
    // An open connected palette validates its bound session even while the
    // fresh HTTP query gate is pending. The gate controls query data only.
    let caller = if open {
        Some(super::session::read(
            cx,
            super::context::caller(cx).and_then(|caller| {
                crate::api::require_user(&caller.identity)?;
                Ok(caller)
            }),
        )?)
    } else {
        None
    };
    let allowed = open && revision == authorized;
    let (projects, issues) = if let Some(caller) = caller.filter(|_| allowed) {
        let projects = crate::services::projects::list_visible_projects(
            super::context::db(cx),
            &caller.identity,
        )?;
        // Only the parsed logical page supplies a current project, never its mount.
        let issues = super::palette_reference::issue_hits(
            super::context::db(cx),
            &caller.identity,
            &query,
            current_project.as_deref(),
        )?;
        (projects, issues)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut rows = issues
        .into_iter()
        .map(|issue| {
            let icon = match issue.status.as_str() {
                "active" => UiIcon::ActiveIssue,
                "todo" => UiIcon::TodoIssue,
                "done" => UiIcon::DoneIssue,
                "cancelled" => UiIcon::CancelledIssue,
                _ => UiIcon::BacklogIssue,
            };
            (
                issue.logical_destination,
                issue.title,
                issue.identifier,
                issue.project_name,
                icon,
            )
        })
        .collect::<Vec<_>>();
    rows.extend(
        matching_projects(&projects, &query)
            .into_iter()
            .map(|project| {
                (
                    format!("/{}/overview", project.identifier),
                    project.name.clone(),
                    project.identifier.clone(),
                    String::new(),
                    UiIcon::Project,
                )
            }),
    );
    let projection = Projection {
        revision,
        destinations: rows
            .iter()
            .map(|row| super::transport::mounted_url(cx, &row.0))
            .collect(),
    };
    let empty_text = if super::palette_reference::parse_reference(&query).is_some() {
        format!("Nothing matches “{}”", query.trim())
    } else {
        "No matching projects".to_owned()
    };
    let selected_style = selected.clone();
    let hover_selected = selected.clone();
    let hover_href = selected_href.clone();
    let hover_cursor = cursor_moved.clone();
    let hover_revision = live_revision.clone();
    let click_open = live_open.clone();
    let click_revision = live_revision.clone();
    let click_enter = pending_enter.clone();
    let click_new_tab = pending_new_tab.clone();
    let all_signals = (
        live_revision,
        selected,
        selected_href,
        cursor_moved,
        count,
        rendered,
        pending_enter,
        pending_new_tab,
        waiting,
        live_open,
        path,
        (live_query.clone(), pending_focus.clone()),
    );
    let projection_key = format!(
        "{}#palette-projection",
        super::shell_handlers::handler_url()
    );
    let arguments = |mode: &str| {
        Js::builder()
            .raw("[")
            .surrogate(&all_signals.clone().into_surrogate())
            .raw(",")
            .surrogate(&projection.clone().into_surrogate())
            .raw(",")
            .surrogate(&allowed.into_surrogate())
            .raw(",")
            .surrogate(&mode.to_owned().into_surrogate())
            .raw("]")
            .build()
    };
    let keyboard = super::handler_asset::event(cx, &projection_key, arguments("key"), "keydown");
    let mut result_mount = Attributes::with_capacity(usize::from(allowed));
    if allowed {
        result_mount = super::handler_asset::mount(cx, &projection_key, arguments("mount"));
    }
    let input_mount = super::handler_asset::mount(cx, &projection_key, arguments("focus"));
    let error = error_signal;
    let waiting_view = all_signals.8.clone();
    Ok(view! {
        <input
            id="native-home-palette-query"
            type="search"
            maxlength="128"
            autocomplete="off"
            :value=$(live_query.get())
            (keyboard)
            (input_mount)
        >
        <p
            class="native-home-palette-error"
            role="alert"
            :hidden=$(error.get().is_empty())
        >
            $(error.get())
        </p>
        <p class="native-home-palette-searching" :hidden=$(!waiting_view.get())>
            "Searching…"
        </p>

        <nav
            class="native-home-palette-results"
            aria-label="Project search results"
            data-native-home-connected=(if connected { "true" } else { "false" })
            (result_mount)
        >
            if allowed && rows.is_empty() {
                <p>(empty_text)</p>
            }
            #[key(destination.clone())]
            for (index, (destination, title, identifier, project_name, icon)) in rows
                .into_iter()
                .enumerate() {
                let mounted_destination = super::transport::mounted_url(cx, &destination);
                <a
                    class="native-home-destination"
                    (super::navigation::attrs(cx, &destination))
                    data-palette-index=(index.to_string())
                    :data-native-palette-selected=$(if selected_style.get() == index {
                        "true"
                    } else {
                        "false"
                    })
                    @mouseenter=$(|_event: Event| {
                        if hover_revision.get() == revision {
                            hover_selected.set(index);
                            hover_href.set(mounted_destination.clone());
                            hover_cursor.set(true);
                        }
                    })
                    @click=$(|_event: Event| {
                        click_open.set(false);
                        click_revision.increment();
                        click_enter.set(false);
                        click_new_tab.set(false);
                    })
                >
                    (super::icons::ui_icon(cx, icon, 16))
                    <span class="native-home-palette-row-copy">
                        <span>(title)</span>
                        if !project_name.is_empty() {
                            <span class="native-home-palette-row-project">
                                (project_name)
                            </span>
                        }
                    </span>
                    <small>(identifier)</small>
                </a>
            }
        </nav>
    })
}
