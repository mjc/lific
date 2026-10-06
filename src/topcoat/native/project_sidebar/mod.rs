//! Authorized project sidebar, with one model and signal owner for both layouts.
mod actions;
mod model;
mod recents_actions;
mod recents_state;
mod recents_view;
mod state;
mod view;

use super::{context, home_shell, session};
use crate::error::LificError;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(crate) fn handler_source() -> &'static str {
    state::handler_source()
}

pub(crate) fn handler_url() -> &'static str {
    state::handler_url()
}

/// Shared projection owner. The shell passes these same handles to its desktop
/// and phone slots; neither wrapper loads a second catalog or owns group drafts.
pub(crate) struct Sidebar {
    signals: state::Signals,
    recents: recents_state::Signals,
    scrolled: Signal<i64>,
}

pub(crate) type SidebarHandles = (i64, state::Handles, recents_state::Handles, Signal<i64>);

impl Sidebar {
    pub(crate) fn handles(&self) -> SidebarHandles {
        (
            self.signals.account,
            self.signals.handles(),
            self.recents.handles(),
            self.scrolled.clone(),
        )
    }

    pub(crate) fn from_handles(handles: SidebarHandles) -> Self {
        let (account, signals, recents, scrolled) = handles;
        Self {
            signals: state::Signals::from_handles(account, signals),
            recents: recents_state::Signals::from_handles(account, recents),
            scrolled,
        }
    }

    pub(crate) fn mount(&self, cx: &Cx) -> Attributes {
        state::mount(cx, &self.signals)
    }

    pub(crate) fn load(cx: &Cx, account: i64, path: &str) -> topcoat::Result<Self> {
        let caller = session::read(cx, context::caller(cx))?;
        let reads = session::read(
            cx,
            crate::services::project_sidebar::load(context::db(cx), &caller.identity),
        )?;
        if reads.user.id != account {
            return Err(
                LificError::Forbidden("Your account changed. Reload this page.".into()).into(),
            );
        }
        let mut model = model::State::new(model::Catalog::from(&reads));
        model.groups_ready = reads.groups_ready;
        model.error = reads.group_error;
        model.reveal(super::super::shell::ParsedRoute::parse(path).project);
        let recents = recents_state::Signals::new(cx, account, &model, path)?;
        Ok(Self {
            signals: state::Signals::new(cx, model)?,
            recents,
            scrolled: signal(cx, || 0_i64),
        })
    }
    /// The sibling action menu owns keyboard and focus above phone navigation.
    pub(crate) fn menu_open(&self) -> topcoat::runtime::Expr<bool> {
        let kind = self.signals.menu_kind.clone();
        topcoat::runtime::expr!(!kind.get().is_empty())
    }
    pub(crate) fn menu_kind(&self) -> Signal<String> {
        self.signals.menu_kind.clone()
    }
    /// Route changes reveal a project once through the existing local transition.
    pub(crate) fn route<'a>(&self, cx: &'a Cx, path: Signal<String>) -> BoxView<'a> {
        let account = self.signals.account;
        let model = self.signals.model.clone();
        let busy = self.signals.busy.clone();
        let frozen = self.signals.frozen.clone();
        let blocked = topcoat::runtime::expr!(if busy.get() {
            true
        } else {
            !frozen.get().is_empty()
        });
        let handles = self.signals.handles();
        view! {cx => native_sidebar_route(account: account, path: $(path.get()), wire: $(model.get()), blocked: $(blocked), handles: handles)}.boxed()
    }
    pub(crate) fn desktop<'a>(&self, cx: &'a Cx, path: Signal<String>) -> BoxView<'a> {
        let account = self.signals.account;
        let handles = self.signals.handles();
        let model = self.signals.model.clone();
        let revision = self.signals.revision.clone();
        let recents = self.recents.handles();
        let catalog = model.clone();
        let storage = recents_state::storage(cx, &self.recents);
        let reveal = (path.clone(), self.scrolled.clone());
        view! {cx => <span hidden="hidden" (storage)></span> recents_view::driver(account:account,path:$(path.get()),handles:recents.clone(),catalog:catalog) native_sidebar_desktop(account:account, wire:$(model.get()), path:$(path.get()), revision:$(revision.get()), handles:handles,recents:recents,reveal:reveal)}.boxed()
    }
    pub(crate) fn phone<'a>(
        &self,
        cx: &'a Cx,
        path: Signal<String>,
        navigation: home_shell::MobileNavigationSignals,
    ) -> BoxView<'a> {
        let account = self.signals.account;
        let handles = self.signals.handles();
        let model = self.signals.model.clone();
        let revision = self.signals.revision.clone();
        let recents = self.recents.handles();
        let initialized = navigation.7.clone();
        view! {cx => native_sidebar_phone(account:account, wire:$(model.get()), path:$(path.get()), revision:$(revision.get()), handles:handles, navigation:navigation,recents:recents, initialized:$(initialized.get()))}.boxed()
    }
    pub(crate) fn phone_panels<'a>(
        &self,
        cx: &'a Cx,
        path: Signal<String>,
        navigation: home_shell::MobileNavigationSignals,
    ) -> BoxView<'a> {
        let account = self.signals.account;
        let handles = self.signals.handles();
        let model = self.signals.model.clone();
        let revision = self.signals.revision.clone();
        let recents = self.recents.handles();
        // Main keeps its last viewed project pane mounted across root/close.
        // Current pane/history admission still uses the live project at index 2.
        let selected = navigation.6.clone();
        let focused_selection = navigation.2.clone();
        // Capture the actual focus source when this selection changes, before
        // its genuine panel request. A later user focus choice must win.
        let empty_focus = String::new();
        let focus_source = expr!({
            if focused_selection.get().is_empty() {
                empty_focus.clone()
            } else {
                raw!(
                    "cx.hydrate(document.activeElement === document.body ? 'body' : document.activeElement?.id || '')",
                    String::new()
                )
            }
        });
        view!{cx=>native_sidebar_phone_panels(account:account,wire:$(model.get()),path:$(path.get()),revision:$(revision.get()),handles:handles,navigation:navigation,recents:recents,selected:$(selected.get()),focus_source:$(focus_source))}.boxed()
    }
    pub(crate) fn menu<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        let account = self.signals.account;
        let handles = self.signals.handles();
        let model = self.signals.model.clone();
        let kind = self.signals.menu_kind.clone();
        let id = self.signals.menu_id.clone();
        let revision = self.signals.revision.clone();
        view! {cx => native_sidebar_menu(account:account, wire:$(model.get()), kind:$(kind.get()), id:$(id.get()), revision:$(revision.get()), handles:handles)}.boxed()
    }
}
fn projection(cx: &Cx, account: i64, wire: &str) -> topcoat::Result<model::State> {
    let (_, model) = session::read(cx, actions::decoded(cx, account, wire))?;
    Ok(model)
}
#[shard("/__native_sidebar/route")]
async fn native_sidebar_route(
    cx: &Cx,
    account: i64,
    path: String,
    wire: String,
    blocked: bool,
    handles: state::Handles,
) -> topcoat::Result<impl View> {
    let mut model = projection(cx, account, &wire)?;
    let previous = model.revealed;
    let identifier = super::super::shell::ParsedRoute::parse(&path).project;
    let revealed = model.reveal(identifier);
    let changed = revealed.is_some() || previous != model.revealed;
    let mount = if !blocked && changed {
        state::invoke(
            cx,
            &state::Signals::from_handles(account, handles),
            "reveal",
            0,
            identifier.unwrap_or_default().to_owned(),
            "mount",
        )
    } else {
        Attributes::with_capacity(0)
    };
    Ok(view! {cx => <span hidden="hidden" (mount)></span>}.boxed())
}

use desktop_shard::native_sidebar_desktop;
#[allow(
    clippy::too_many_arguments,
    reason = "One native shard expands an implicit context plus separate reactive owner inputs"
)]
mod desktop_shard {
    use super::*;

    #[shard("/__native_sidebar/desktop")]
    pub(super) async fn native_sidebar_desktop(
        cx: &Cx,
        account: i64,
        wire: String,
        path: String,
        revision: usize,
        handles: state::Handles,
        recents: recents_state::Handles,
        reveal: (Signal<String>, Signal<i64>),
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let model = projection(cx, account, &wire)?;
        let signals = state::Signals::from_handles(account, handles);
        let identifier = super::super::super::shell::ParsedRoute::parse(&path).project;
        let current = identifier
            .and_then(|identifier| {
                model
                    .catalog
                    .projects
                    .iter()
                    .find(|project| project.identifier.eq_ignore_ascii_case(identifier))
            })
            .map(|project| project.id);
        let target = current
            .filter(|id| model.groups_ready && model.revealed == Some(*id))
            .unwrap_or(0);
        let reset = current.is_none();
        let current_path = reveal.0;
        let scrolled = reveal.1;
        let current_revision = signals.revision.clone();
        let expected_path = path.clone();
        // The mounted shard is the actual adopted desktop projection. A late
        // frame cannot reveal another route or an obsolete model snapshot.
        let mounted = expr!(|_event: Event| {
            let _scroll = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    if current_path.get() == expected_path {
                        if current_revision.get() == revision {
                            if reset {
                                scrolled.set(0_i64);
                            } else {
                                if target > 0_i64 {
                                    if scrolled.get() != target {
                                        scrolled.set(target);
                                        if raw!(
                                            "cx.hydrate(matchMedia('(min-width: 768px)').matches)",
                                            false
                                        ) {
                                            raw!(
                                                "document.getElementById('native-sidebar-project-link-'+${target}.toString())?.scrollIntoView({block:'nearest'});",
                                                ()
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            };
            raw!("requestAnimationFrame(() => ${_scroll}());", ());
        });
        let projects = view::projects(
            cx,
            &model,
            &signals,
            &path,
            view::Layout::Desktop,
            &|_| Attributes::with_capacity(0),
            &recents_state::Signals::from_handles(account, recents),
        );
        let restore = state::restore_region_focus(
            cx,
            &signals,
            revision,
            "desktop".to_owned(),
            model.edit.is_none(),
        );
        Ok(view! {cx => <span hidden="hidden" @mount=(mounted)></span><span hidden="hidden" (restore)></span>(projects)}.boxed())
    }
}
use phone_shard::native_sidebar_phone;
#[allow(
    clippy::too_many_arguments,
    reason = "One native shard expands an implicit context plus separate reactive owner inputs"
)]
mod phone_shard {
    use super::*;

    #[shard("/__native_sidebar/phone")]
    pub(super) async fn native_sidebar_phone(
        cx: &Cx,
        account: i64,
        wire: String,
        path: String,
        revision: usize,
        handles: state::Handles,
        navigation: home_shell::MobileNavigationSignals,
        recents: recents_state::Handles,
        initialized: bool,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let model = projection(cx, account, &wire)?;
        let signals = state::Signals::from_handles(account, handles);
        let navigation = home_shell::MobileNavigation::from_handles(navigation);
        let restore = if initialized {
            state::restore_region_focus(
                cx,
                &signals,
                revision,
                "phone".to_owned(),
                model.edit.is_none(),
            )
        } else {
            Attributes::with_capacity(0)
        };
        let projects = if initialized {
            view::projects(
                cx,
                &model,
                &signals,
                &path,
                view::Layout::Phone,
                &|identifier| {
                    home_shell::mobile_action(cx, &navigation, "project", identifier.to_owned())
                },
                &recents_state::Signals::from_handles(account, recents),
            )
        } else {
            view! {cx=>}.boxed()
        };
        Ok(view! {cx => <span hidden="hidden" (restore)></span>(projects)}.boxed())
    }
}
use menu_shard::native_sidebar_menu;
#[allow(
    clippy::too_many_arguments,
    reason = "One native shard expands an implicit context plus separate reactive owner inputs"
)]
mod menu_shard {
    use super::*;

    #[shard("/__native_sidebar/menu")]
    pub(super) async fn native_sidebar_menu(
        cx: &Cx,
        account: i64,
        wire: String,
        kind: String,
        id: i64,
        revision: usize,
        handles: state::Handles,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let (_, model) = session::read(cx, actions::decoded(cx, account, &wire))?;
        let signals = state::Signals::from_handles(account, handles);
        Ok(view! {cx=>if !kind.is_empty(){(view::menu(cx,&model,&signals,&kind,id))}}.boxed())
    }
}

use phone_panels_shard::native_sidebar_phone_panels;
#[allow(
    clippy::too_many_arguments,
    reason = "One native shard expands an implicit context plus separate reactive owner inputs"
)]
mod phone_panels_shard {
    use super::*;

    #[shard("/__native_sidebar/phone_panels")]
    pub(super) async fn native_sidebar_phone_panels(
        cx: &Cx,
        account: i64,
        wire: String,
        path: String,
        revision: usize,
        handles: state::Handles,
        navigation: home_shell::MobileNavigationSignals,
        recents: recents_state::Handles,
        selected: String,
        focus_source: String,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let mut model = projection(cx, account, &wire)?;
        // The root still renders every authorized project. Only its currently
        // selected destination pane is materialized, matching MobileNav.
        model
            .catalog
            .projects
            .retain(|project| project.identifier == selected);
        let focus_trigger = model
            .catalog
            .projects
            .first()
            .map_or_else(String::new, |project| {
                format!("native-sidebar-phone-project-{}", project.id)
            });
        let signals = state::Signals::from_handles(account, handles);
        let current_revision = signals.revision;
        let menu = signals.menu_kind;
        let navigation = home_shell::MobileNavigation::from_handles(navigation);
        let (open, pane, current_selected, _, _, pending_palette, _, _) = navigation.handles();
        let mounted = expr!(|_event: Event| {
            let _focus = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    if current_selected.get() == selected {
                        if current_revision.get() == revision {
                            if open.get() {
                                if pane.get() == "project" {
                                    if menu.get().is_empty() {
                                        if !pending_palette.get() {
                                            let other_surface = raw!(
                                                "cx.hydrate(Boolean(document.querySelector('.native-home-palette-backdrop:not([hidden]),.native-home-theme-menu:not([hidden])')))",
                                                false
                                            );
                                            if !other_surface {
                                                let actual_focus = raw!(
                                                    "cx.hydrate(document.activeElement === document.body ? 'body' : document.activeElement?.id || '')",
                                                    String::new()
                                                );
                                                // Hiding the clicked root row automatically
                                                // blurs it to body before its panel arrives.
                                                let source_blurred = if actual_focus == "body" {
                                                    if focus_source == focus_trigger {
                                                        if !focus_trigger.is_empty() {
                                                            raw!(
                                                                "cx.hydrate(Boolean(document.getElementById(${focus_trigger}.toString())?.closest('[hidden],[inert]')))",
                                                                false
                                                            )
                                                        } else {
                                                            false
                                                        }
                                                    } else {
                                                        false
                                                    }
                                                } else {
                                                    false
                                                };
                                                let unchanged = if actual_focus == focus_source {
                                                    true
                                                } else {
                                                    source_blurred
                                                };
                                                if unchanged {
                                                    let admitted = if focus_source == "body" {
                                                        true
                                                    } else {
                                                        if focus_source == "native-home-mobile-open"
                                                        {
                                                            true
                                                        } else {
                                                            if !focus_trigger.is_empty() {
                                                                focus_source == focus_trigger
                                                            } else {
                                                                false
                                                            }
                                                        }
                                                    };
                                                    if admitted {
                                                        raw!(
                                                            "document.getElementById('native-mobile-project-'+${selected}.toString())?.querySelector('button')?.focus();",
                                                            ()
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            };
            raw!("requestAnimationFrame(() => ${_focus}());", ());
        });
        let panels = view::phone_panels(
            cx,
            &model,
            &path,
            &navigation,
            &recents_state::Signals::from_handles(account, recents),
        );
        Ok(view! {cx => <span hidden="hidden" @mount=(mounted)></span> (panels)}.boxed())
    }
}

#[cfg(test)]
mod production;

#[cfg(test)]
mod disclosure_production;

#[cfg(test)]
mod order_production;

#[cfg(test)]
mod expiry_production;

mod recents_model;

mod receipts;
pub(crate) use receipts::SidebarWriteStore;

#[cfg(test)]
mod recents_focus_production;
