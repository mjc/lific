//! The native phone drawer's view, owned history state, and browser actions.

use super::super::runtime::signal_vec::VecPositionExt;
use super::icons::UiIcon;
use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, Surrogated, expr, record, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

#[derive(Clone)]
pub(crate) struct MobileNavigation {
    pub(super) open: Signal<bool>,
    pub(super) pane: Signal<String>,
    pub(super) project: Signal<String>,
    pub(super) owner: Signal<String>,
    pub(super) href: Signal<String>,
    pub(super) pending_palette: Signal<bool>,
    // Keep the last project parked while the root pane is presented.
    pub(super) view_identifier: Signal<String>,
    // The deferred phone shard stays mounted after its first use.
    pub(super) initialized: Signal<bool>,
}

pub(crate) type MobileNavigationSignals = (
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
    Signal<bool>,
);

impl MobileNavigation {
    pub(crate) fn new(cx: &Cx) -> Self {
        Self {
            open: signal(cx, || false),
            pane: signal(cx, || "root".to_owned()),
            project: signal(cx, String::new),
            owner: signal(cx, String::new),
            href: signal(cx, String::new),
            pending_palette: signal(cx, || false),
            view_identifier: signal(cx, String::new),
            initialized: signal(cx, || false),
        }
    }

    pub(crate) fn handles(&self) -> MobileNavigationSignals {
        (
            self.open.clone(),
            self.pane.clone(),
            self.project.clone(),
            self.owner.clone(),
            self.href.clone(),
            self.pending_palette.clone(),
            self.view_identifier.clone(),
            self.initialized.clone(),
        )
    }

    pub(crate) fn from_handles(handles: MobileNavigationSignals) -> Self {
        Self {
            open: handles.0,
            pane: handles.1,
            project: handles.2,
            owner: handles.3,
            href: handles.4,
            pending_palette: handles.5,
            view_identifier: handles.6,
            initialized: handles.7,
        }
    }
}

#[record]
#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryEntry {
    version: String,
    owner: String,
    href: String,
    pane: String,
    project: String,
}

pub(super) type ChromeHandlerSignals<'a> = (
    &'a Signal<bool>,
    &'a Signal<String>,
    &'a Signal<bool>,
    &'a Signal<bool>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<bool>,
    &'a Signal<String>,
    &'a Signal<bool>,
    &'a Signal<String>,
);

#[shard("/__native_home/phone")]
pub(super) async fn native_home_phone(
    cx: &Cx,
    initialized: bool,
    sidebar: super::project_sidebar::SidebarHandles,
    navigation: (Signal<String>, MobileNavigationSignals),
    theme_controls: (Signal<String>, Signal<bool>),
    profile: Option<super::account_profile::Handles>,
) -> topcoat::Result<impl View> {
    if !initialized {
        return Ok(view! { cx => }.boxed());
    }
    let account = sidebar.0;
    let (path, navigation) = navigation;
    let (theme, theme_menu) = theme_controls;
    let caller = super::session::read(cx, super::context::caller(cx))?;
    let user = super::session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let display_name =
        super::avatar::display_name(Some(&user.display_name), Some(&user.username), "").to_owned();
    let initials = super::home_shell::display_initials(&user.display_name, &user.username);
    let account_link = profile.map_or_else(
        || super::home_shell::account_link(cx, display_name, initials, true),
        |handles| super::account_profile::link(cx, handles, true),
    );
    let sidebar = super::project_sidebar::Sidebar::from_handles(sidebar);
    let sidebar_menu_open = sidebar.menu_open();
    let navigation = MobileNavigation::from_handles(navigation);
    let mobile_open = navigation.open.clone();
    let mobile_pane = navigation.pane.clone();
    let home_active = expr!(if path.get() == "/" {
        true
    } else {
        path.get().starts_with("/?")
    });
    let focus_open = mobile_open.clone();
    let focus_pane = mobile_pane.clone();
    let focus_theme = theme_menu.clone();
    let focus_sidebar = sidebar_menu_open.clone();
    let focus_pending = navigation.pending_palette.clone();
    // The first-use opener runs before this deferred shard has mounted.
    let phone_mount = expr!(|_event: Event| {
        let _focus = || {
            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                if focus_open.get() {
                    if !focus_theme.get() {
                        if !focus_sidebar {
                            if !focus_pending.get() {
                                if focus_pane.get() == "root" {
                                    raw!(
                                        "document.querySelector('[data-native-mobile-root] button')?.focus();",
                                        ()
                                    );
                                } else if focus_pane.get() == "unavailable" {
                                    raw!(
                                        "document.querySelector('[data-native-mobile-unavailable] button')?.focus();",
                                        ()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        };
        raw!("queueMicrotask(() => ${_focus}());", ());
    });
    Ok(view! {
        cx =>
        <section
            data-native-mobile-nav=""
            role="dialog"
            :aria-modal=$(if sidebar_menu_open {
                "false"
            } else {
                if theme_menu.get() { "false" } else { "true" }
            })
            aria-label="Workspace navigation"
            :hidden=$(!mobile_open.get())
            @mount=(phone_mount)
        >
            <div data-native-mobile-root="" :hidden=$(mobile_pane.get() != "root")>
                <header class="native-home-mobile-nav-header">
                    <img
                        src=(super::preloads::image_url(cx, "/logo.webp"))
                        alt=""
                        width="28"
                        height="28"
                    />
                    <strong>"Lific"</strong>
                    <small>(concat!("v", env!("CARGO_PKG_VERSION")))</small>
                    <button
                        class="native-home-icon-button"
                        aria-label="Close navigation"
                        (mobile_action(cx, &navigation, "close", String::new()))
                    >
                        (super::icons::ui_icon(cx, UiIcon::Close, 20))
                    </button>
                </header>
                <button
                    class="native-home-mobile-search"
                    (mobile_action(cx, &navigation, "search", String::new()))
                >
                    (super::icons::ui_icon(cx, UiIcon::Search, 18))
                    "Search issues, pages, projects…"
                </button>
                <nav aria-label="Phone workspace">
                    <a
                        class="native-home-mobile-link"
                        (super::navigation::attrs(cx, "/"))
                        :aria-current=$(home_active.then_some("page"))
                    >
                        (super::icons::ui_icon(cx, UiIcon::Home, 20))
                        "Home"
                    </a>
                    (sidebar.phone(cx, path.clone(), navigation.handles()))
                </nav>
                <footer class="native-home-mobile-footer">
                    (account_link)
                    (super::home_shell::theme_button(
                        cx,
                        theme.clone(),
                        theme_menu.clone(),
                    ))
                </footer>
            </div>
            (sidebar.phone_panels(cx, path.clone(), navigation.handles()))
            (mobile_unavailable_panel(cx, &navigation))
        </section>
    }
    .boxed())
}

fn mobile_unavailable_panel<'a>(cx: &'a Cx, navigation: &MobileNavigation) -> BoxView<'a> {
    let pane = navigation.pane.clone();
    let back = mobile_action(cx, navigation, "back", String::new());
    let close = mobile_action(cx, navigation, "close", String::new());
    view! {
        cx =>
        <div data-native-mobile-unavailable="" :hidden=$(pane.get() != "unavailable")>
            <header
                class="native-home-mobile-nav-header native-home-unavailable-header"
            >
                <button
                    class="native-home-icon-button native-home-unavailable-back"
                    aria-label="Back to projects"
                    (back)
                >
                    (super::icons::ui_icon(cx, UiIcon::Previous, 20))
                    "Projects"
                </button>
                <button
                    class="native-home-icon-button"
                    aria-label="Close navigation"
                    (close)
                >
                    (super::icons::ui_icon(cx, UiIcon::Close, 20))
                </button>
            </header>
            <div class="native-home-unavailable-copy">
                <h2>"Project unavailable"</h2>
                <p>"This project is no longer in your project list."</p>
            </div>
        </div>
    }
    .boxed()
}

pub(crate) fn mobile_action(
    cx: &Cx,
    _navigation: &MobileNavigation,
    action: &str,
    identifier: String,
) -> Attributes {
    let encoded = serde_json::to_string(&(action, identifier))
        .expect("Mobile action arguments contain only serializable scalar values");
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-native-mobile-action", encoded);
    attributes
}

pub(crate) fn mobile_action_mount(cx: &Cx, navigation: &MobileNavigation) -> Attributes {
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&(
            (&navigation.open).into_surrogate(),
            (&navigation.pane).into_surrogate(),
            (&navigation.project).into_surrogate(),
            (&navigation.owner).into_surrogate(),
            (&navigation.href).into_surrogate(),
            (&navigation.pending_palette).into_surrogate(),
            (&navigation.view_identifier).into_surrogate(),
            (&navigation.initialized).into_surrogate(),
        ))
        .raw("]")
        .build();
    let key = format!("{}#mobile-dispatch", super::home_shell::handler_url());
    let mut attributes = super::handler_asset::mount(cx, &key, arguments);
    attributes.insert(cx, "id", "native-mobile-action-owner");
    attributes
}

type MobileHandlerSignals<'a> = (
    &'a Signal<bool>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<String>,
    &'a Signal<bool>,
    &'a Signal<String>,
    &'a Signal<bool>,
);

pub(crate) fn dispatch_factory() -> Js {
    let handler = expr!(|browser: super::browser::Browser,
                         _event: Event,
                         handles: MobileHandlerSignals<'_>| {
        let open = handles.0;
        let pane = handles.1;
        let project = handles.2;
        let owner = handles.3;
        let href = handles.4;
        let pending_palette = handles.5;
        let view_identifier = handles.6;
        let initialized = handles.7;
        let _dispatch = |action: String, identifier: String| {
            if !pending_palette.get() {
                if action == "back" {
                    raw!("history.back();", ());
                } else if action == "search" {
                    pending_palette.set(true);
                    open.set(false);
                    if pane.get() == "root" {
                        raw!("history.back();", ());
                    } else {
                        raw!("history.go(-2);", ());
                    }
                } else if action == "close" {
                    if pane.get() == "root" {
                        raw!("history.back();", ());
                    } else {
                        raw!("history.go(-2);", ());
                    }
                } else if action == "open" {
                    let _owner = owner.get();
                    let _href = href.get();
                    raw!(
                        "history.replaceState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:'closed',project:''}},'');",
                        ()
                    );
                    raw!(
                        "history.pushState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:'root',project:''}},'');",
                        ()
                    );
                    project.set("".to_owned());
                    pane.set("root".to_owned());
                    if !initialized.get() {
                        initialized.set(true);
                    }
                    open.set(true);
                    let selector = "[data-native-mobile-root] button";
                    browser.microtask(|| browser.focus_selector(selector.to_owned()));
                } else if action == "project" {
                    let _owner = owner.get();
                    let _href = href.get();
                    raw!(
                        "history.pushState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:'project',project:${identifier}.toString()}},'');",
                        ()
                    );
                    view_identifier.set(identifier.clone());
                    project.set(identifier.clone());
                    pane.set("project".to_owned());
                    if !initialized.get() {
                        initialized.set(true);
                    }
                    open.set(true);
                    let selector = "[data-native-mobile-project]:not([hidden]) button";
                    browser.microtask(|| browser.focus_selector(selector.to_owned()));
                }
            }
        };
        raw!(
            r#"const root=document.getElementById('native-mobile-action-owner')?.closest('.native-home-shell');
            if(!root)return;
            root.addEventListener('click',event=>{
                const target=event.target instanceof Element?event.target:event.target?.parentElement;
                const node=target?.closest('[data-native-mobile-action]');
                if(!node||!root.contains(node)||node.closest('.native-home-shell')!==root)return;
                const args=JSON.parse(node.getAttribute('data-native-mobile-action'));
                ${_dispatch}(cx.hydrate(args[0]),cx.hydrate(args[1]));
            },{signal:cx.abortSignal});"#,
            ()
        );
    });
    let body = handler.into_evaluated_and_js().1;
    Js::builder()
        .source("(event,...args)=>{const browser=")
        .expression(&super::browser::bindings())
        .source(";return (")
        .source(body.to_source())
        .source(")(browser,event,...args)}")
        .build()
}

/// Generate the drawer's own traversal, focus, resize, and navigation guard.
/// Palette work remains owned by the shell and is passed as one callback.
pub(crate) fn handler_factory() -> Js {
    let handler = expr!(|browser: super::browser::Browser,
                         _mount: Event,
                         chrome: ChromeHandlerSignals<'_>,
                         projects: &Vec<String>,
                         palette: &super::palette::OpenPalette| {
        let theme_menu = chrome.2;
        let mobile_open = chrome.3;
        let mobile_pane = chrome.4;
        let mobile_project = chrome.5;
        let owner = chrome.6;
        let href = chrome.7;
        let pending_palette = chrome.8;
        let view_identifier = chrome.9;
        let initialized = chrome.10;
        let sidebar_menu = chrome.11;
        // Pending browser actions belong to the mounting scope, never its replacement.
        pending_palette.set(false);

        // Decode the browser record as one typed value at the boundary.
        let _read_entry = || {
            raw!(
                r#"cx.record((() => {
                    const record=history.state?.lificNativeHomeNav;
                    const value=record&&typeof record==='object'?record:{};
                    return {
                        version:cx.hydrate(typeof value.version==='string'?value.version:''),
                        owner:cx.hydrate(typeof value.owner==='string'?value.owner:''),
                        href:cx.hydrate(typeof value.href==='string'?value.href:''),
                        pane:cx.hydrate(typeof value.pane==='string'?value.pane:''),
                        project:cx.hydrate(typeof value.project==='string'?value.project:'')
                    };
                })())"#,
                HistoryEntry {
                    version: "".to_owned(),
                    owner: "".to_owned(),
                    href: "".to_owned(),
                    pane: "".to_owned(),
                    project: "".to_owned(),
                }
            )
        };
        let _owns_closed_entry = |record: HistoryEntry,
                                  expected_owner: String,
                                  expected_href: String,
                                  current_href: String| {
            if record.version == "1" {
                if record.owner == expected_owner {
                    if record.href == expected_href {
                        if current_href == expected_href {
                            if record.pane == "closed" {
                                record.project.is_empty()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };
        let _focus_entry = |previous: String| {
            if mobile_pane.get() == "project" {
                browser
                    .focus_selector("[data-native-mobile-project]:not([hidden]) button".to_owned());
            } else if mobile_pane.get() == "unavailable" {
                browser.focus_selector("[data-native-mobile-unavailable] button".to_owned());
            } else {
                browser.focus_project(previous);
            }
        };
        let _present = |arguments: (bool, HistoryEntry)| {
            let history_pop = arguments.0;
            let record = arguments.1;
            let current_href = raw!("cx.hydrate(window.location.href)", "".to_owned());
            let history_owner = owner.get();
            let history_href = href.get();
            let entry_shape_valid = if record.pane == "closed" {
                record.project.is_empty()
            } else if record.pane == "root" {
                record.project.is_empty()
            } else {
                if record.pane == "project" {
                    !record.project.is_empty()
                } else {
                    false
                }
            };
            let entry_owned = if record.version == "1" {
                if record.owner == history_owner {
                    if record.href == history_href {
                        if current_href == history_href {
                            entry_shape_valid
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if history_pop {
                if record.version == "1" {
                    if record.owner == history_owner {
                        if current_href == record.href {
                            let restored_shape = if record.pane == "closed" {
                                record.project.is_empty()
                            } else if record.pane == "root" {
                                record.project.is_empty()
                            } else {
                                if record.pane == "project" {
                                    !record.project.is_empty()
                                } else {
                                    false
                                }
                            };
                            if restored_shape {
                                href.set(record.href.clone());
                            }
                        }
                    }
                }
            }
            let owned = if history_pop {
                if record.version == "1" {
                    if record.owner == owner.get() {
                        if record.href == href.get() {
                            if current_href == href.get() {
                                entry_shape_valid
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                entry_owned
            };
            let was_open = mobile_open.get();
            let previous = mobile_project.get();
            let desktop = browser.media("(min-width: 768px)".to_owned());
            let presented = if desktop {
                "closed"
            } else {
                if owned {
                    if record.pane == "root" {
                        "root"
                    } else {
                        if record.pane == "project" {
                            if projects.position(record.project.clone()).is_some() {
                                "project"
                            } else {
                                "unavailable"
                            }
                        } else {
                            "closed"
                        }
                    }
                } else {
                    "closed"
                }
            };
            if presented == "closed" {
                mobile_open.set(false);
                mobile_pane.set("root".to_owned());
                if !previous.is_empty() {
                    mobile_project.set("".to_owned());
                }
                if was_open {
                    browser.microtask(|| {
                        browser.focus_id("native-home-mobile-open".to_owned());
                    });
                }
            } else {
                if record.pane == "project" {
                    view_identifier.set(record.project.clone());
                }
                if !initialized.get() {
                    initialized.set(true);
                }
                mobile_open.set(true);
                mobile_pane.set(presented.to_owned());
                if presented == "project" {
                    mobile_project.set(record.project);
                } else {
                    mobile_project.set("".to_owned());
                }
                browser.microtask(|| browser.call1(_focus_entry, previous.clone()));
            }
            if desktop {
                if owned {
                    if record.pane == "root" {
                        raw!("history.back();", ());
                    } else if record.pane == "project" {
                        raw!("history.go(-2);", ());
                    }
                }
            }
            if pending_palette.get() {
                if history_pop {
                    pending_palette.set(false);
                    if owned {
                        if record.pane == "closed" {
                            palette.call("native-home-mobile-open".to_owned());
                        }
                    }
                } else if !owned {
                    pending_palette.set(false);
                }
            }
        };
        let _history = |_event: Event| {
            let _pop = _event.event_type == "popstate";
            let entry = browser.call0(_read_entry);
            browser.call1(_present, (_pop, entry));
        };
        let _resize = |_event: Event| {
            let desktop = browser.media("(min-width: 768px)".to_owned());
            if desktop {
                if mobile_open.get() {
                    mobile_open.set(false);
                    if mobile_pane.get() == "root" {
                        raw!("history.back();", ());
                    } else {
                        raw!("history.go(-2);", ());
                    }
                }
            }
        };
        let _keyboard = |_event: Event| {
            if sidebar_menu.get().is_empty() {
                let key = _event.key.clone();
                if key == "Escape" {
                    if theme_menu.get() {
                        theme_menu.set(false);
                    } else if mobile_open.get() {
                        _event.prevent_default();
                        raw!("history.back();", ());
                    }
                } else if key == "Tab" {
                    if mobile_open.get() {
                        if !theme_menu.get() {
                            let _pane_selector = if mobile_pane.get() == "root" {
                                "[data-native-mobile-root]"
                            } else if mobile_pane.get() == "unavailable" {
                                "[data-native-mobile-unavailable]"
                            } else {
                                "[data-native-mobile-project]:not([hidden])"
                            };
                            let _backwards = _event.shift_key;
                            raw!(
                                r#"(() => {
                        const pane=document.querySelector(${_pane_selector}.toString());
                        if(!pane)return;
                        const items=Array.from(pane.querySelectorAll('button:not(:disabled),a[href],input:not(:disabled),[tabindex="0"]')).filter(element=>element.getClientRects().length&&!element.closest('[inert]'));
                        const first=items[0],last=items.at(-1),active=document.activeElement;
                        if(${_backwards}.dehydrate()?active===first||!pane.contains(active):active===last||!pane.contains(active)){
                            ${_event}.prevent_default();(${_backwards}.dehydrate()?last:first)?.focus();
                        }
                    })();"#,
                                ()
                            );
                        }
                    }
                }
            }
        };
        let _focus = |_event: Event| {
            if sidebar_menu.get().is_empty() {
                if mobile_open.get() {
                    if !theme_menu.get() {
                        let _pane_selector = if mobile_pane.get() == "root" {
                            "[data-native-mobile-root]"
                        } else if mobile_pane.get() == "unavailable" {
                            "[data-native-mobile-unavailable]"
                        } else {
                            "[data-native-mobile-project]:not([hidden])"
                        };
                        let focus_selector = if mobile_pane.get() == "root" {
                            "[data-native-mobile-root] button".to_owned()
                        } else if mobile_pane.get() == "unavailable" {
                            "[data-native-mobile-unavailable] button".to_owned()
                        } else {
                            "[data-native-mobile-project]:not([hidden]) button".to_owned()
                        };
                        let inside = raw!(
                            "cx.hydrate(document.querySelector(${_pane_selector}.toString())?.contains(${_event}.inner.target)||false)",
                            false
                        );
                        if !inside {
                            browser.focus_selector(focus_selector);
                        }
                    }
                }
            }
        };
        let _before_navigation_commit = |_event: Event| {
            let traversal = raw!(
                "cx.hydrate(${_event}.inner.detail.mode === 'traverse')",
                false
            );
            if !traversal {
                if mobile_open.get() {
                    let record = browser.call0(_read_entry);
                    let current_href = raw!("cx.hydrate(window.location.href)", "".to_owned());
                    let _drawer_owner = owner.get();
                    let _drawer_href = href.get();
                    let pane_valid = if record.pane == "closed" {
                        record.project.is_empty()
                    } else if record.pane == "root" {
                        record.project.is_empty()
                    } else {
                        if record.pane == "project" {
                            !record.project.is_empty()
                        } else {
                            false
                        }
                    };
                    let displayed_pane_matches = if mobile_pane.get() == "root" {
                        if record.pane == "root" {
                            record.project.is_empty()
                        } else {
                            false
                        }
                    } else if mobile_pane.get() == "project" {
                        if record.pane == "project" {
                            record.project == mobile_project.get()
                        } else {
                            false
                        }
                    } else if mobile_pane.get() == "unavailable" {
                        if record.pane == "project" {
                            record.project == view_identifier.get()
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    let owned = if record.version == "1" {
                        if record.owner == _drawer_owner {
                            if record.href == _drawer_href {
                                if current_href == _drawer_href {
                                    if pane_valid {
                                        displayed_pane_matches
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if owned {
                        let _steps = if record.pane == "root" {
                            -1_i32
                        } else {
                            -2_i32
                        };
                        raw!(
                            r#"(() => {
                        const event=${_event}.inner,signal=event.detail.signal,root=document.querySelector('.native-home-shell');
                        if(!root){event.detail.waitUntil(Promise.reject(new Error('navigation owner ended')));return;}
                        let work=root.__topcoatDrawerUnwind;
                        if(!work){
                            work=new Promise((resolve,reject)=>{
                                let timer;
                                const ownerSignal=cx.abortSignal;
                                const finish=error=>{clearTimeout(timer);window.removeEventListener('popstate',onPop);ownerSignal.removeEventListener('abort',onOwnerAbort);error?reject(error):resolve();};
                                const onPop=()=>{const valid=${_owns_closed_entry}(${_read_entry}(),${_drawer_owner},${_drawer_href},cx.hydrate(location.href)).dehydrate();finish(valid?null:new Error('drawer history changed'));};
                                const onOwnerAbort=()=>finish(new DOMException('Navigation owner ended','AbortError'));
                                window.addEventListener('popstate',onPop,{once:true});ownerSignal.addEventListener('abort',onOwnerAbort,{once:true});
                                timer=setTimeout(()=>finish(new Error('drawer history did not unwind')),5000);history.go(${_steps});
                            });
                            root.__topcoatDrawerUnwind=work;
                            work.finally(()=>{if(root.__topcoatDrawerUnwind===work)root.__topcoatDrawerUnwind=null;}).catch(()=>{});
                        }
                        event.detail.waitUntil(new Promise((resolve,reject)=>{
                            if(signal.aborted){reject(new DOMException('Navigation cancelled','AbortError'));return;}
                            const aborted=()=>reject(new DOMException('Navigation cancelled','AbortError'));
                            signal.addEventListener('abort',aborted,{once:true});
                            work.then(value=>{signal.removeEventListener('abort',aborted);resolve(value)},error=>{signal.removeEventListener('abort',aborted);reject(error)});
                        }));
                    })();"#,
                            ()
                        );
                    } else {
                        raw!(
                            "${_event}.inner.detail.waitUntil(Promise.reject(new Error('drawer history entry is no longer owned')));",
                            ()
                        );
                    }
                }
            }
        };
        let initial_entry = browser.call0(_read_entry);
        let current_href = raw!("cx.hydrate(window.location.href)", "".to_owned());
        let restore_valid = if initial_entry.version == "1" {
            if !initial_entry.owner.is_empty() {
                if initial_entry.href == current_href {
                    if initial_entry.pane == "closed" {
                        initial_entry.project.is_empty()
                    } else if initial_entry.pane == "root" {
                        initial_entry.project.is_empty()
                    } else if initial_entry.pane == "project" {
                        !initial_entry.project.is_empty()
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if restore_valid {
            owner.set(initial_entry.owner.clone());
        } else {
            owner.set(raw!("cx.hydrate(Array.from(crypto.getRandomValues(new Uint8Array(16)), byte=>byte.toString(16).padStart(2,'0')).join(''))", "".to_owned()));
        }
        href.set(current_href);
        browser.call1(_present, (false, initial_entry));
        browser.window_listener("keydown".to_owned(), _keyboard);
        browser.window_listener("focusin".to_owned(), _focus);
        browser.window_listener("popstate".to_owned(), _history);
        browser.window_listener("hashchange".to_owned(), _history);
        browser.document_listener(
            "topcoat:before-navigation-commit".to_owned(),
            _before_navigation_commit,
        );
        browser.media_listener("(min-width: 768px)".to_owned(), _resize);
    });
    let body = handler.into_evaluated_and_js().1;
    Js::builder()
        .source("(event,...args)=>{const browser=")
        .expression(&super::browser::bindings())
        .source(";return (")
        .source(body.to_source())
        .source(")(browser,event,...args)}")
        .build()
}
