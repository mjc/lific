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
    runtime::{Signal, shard},
    view::{Attributes, BoxView, View, ViewExt, view},
};

/// Shared projection owner. The shell passes these same handles to its desktop
/// and phone slots; neither wrapper loads a second catalog or owns group drafts.
pub(crate) struct Sidebar {
    signals: state::Signals,
    recents: recents_state::Signals,
}
impl Sidebar {
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
        })
    }
    pub(crate) fn desktop<'a>(&self, cx: &'a Cx, path: Signal<String>) -> BoxView<'a> {
        let account = self.signals.account;
        let handles = self.signals.handles();
        let model = self.signals.model.clone();
        let revision = self.signals.revision.clone();
        let recents = self.recents.handles();
        let catalog = model.clone();
        let storage = recents_state::storage(cx, &self.recents);
        view! {cx => <span hidden="hidden" (storage)></span> recents_view::driver(account:account,path:$(path.get()),handles:recents.clone(),catalog:catalog) native_sidebar_desktop(account:account, wire:$(model.get()), path:$(path.get()), revision:$(revision.get()), handles:handles,recents:recents)}.boxed()
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
        view! {cx => native_sidebar_phone(account:account, wire:$(model.get()), path:$(path.get()), revision:$(revision.get()), handles:handles, navigation:navigation,recents:recents)}.boxed()
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
        view!{cx=>native_sidebar_phone_panels(account:account,wire:$(model.get()),path:$(path.get()),revision:$(revision.get()),handles:handles,navigation:navigation,recents:recents)}.boxed()
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
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let model = projection(cx, account, &wire)?;
        let signals = state::Signals::from_handles(account, handles);
        Ok(view::projects(
            cx,
            &model,
            &signals,
            &path,
            view::Layout::Desktop,
            &|_| Attributes::with_capacity(0),
            &recents_state::Signals::from_handles(account, recents),
        ))
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
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let model = projection(cx, account, &wire)?;
        let signals = state::Signals::from_handles(account, handles);
        let navigation = home_shell::MobileNavigation::from_handles(navigation);
        Ok(view::projects(
            cx,
            &model,
            &signals,
            &path,
            view::Layout::Phone,
            &|identifier| {
                home_shell::mobile_action(cx, &navigation, "project", identifier.to_owned())
            },
            &recents_state::Signals::from_handles(account, recents),
        ))
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
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let _ = handles;
        let model = projection(cx, account, &wire)?;
        let navigation = home_shell::MobileNavigation::from_handles(navigation);
        Ok(view::phone_panels(
            cx,
            &model,
            &path,
            &navigation,
            &recents_state::Signals::from_handles(account, recents),
        ))
    }
}

#[cfg(test)]
mod production;

#[cfg(test)]
mod expiry_production;

mod recents_model;

mod receipts;
pub(crate) use receipts::SidebarWriteStore;

#[cfg(test)]
mod recents_focus_production;
