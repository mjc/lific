//! Production Home. Application reads and presentation run in Rust.

use super::icons::UiIcon;
use topcoat::{
    context::{Cx, app_context},
    runtime::{Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

use super::super::runtime::connected;
use super::{
    home_data, home_live, home_local, home_model, home_sections, home_shell, home_view,
    transport::mounted_url,
};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-page.css");

pub(crate) fn authorize(cx: &Cx) -> topcoat::Result<()> {
    super::session::read(
        cx,
        super::context::caller(cx).and_then(|caller| crate::api::require_user(&caller.identity)),
    )
    .map(|_| ())
}

pub(super) fn authorized_snapshot(cx: &Cx) -> topcoat::Result<home_data::Snapshot> {
    super::session::read(cx, home_data::snapshot(cx))
}

pub(crate) fn screen(cx: &Cx) -> topcoat::Result<BoxView<'_>> {
    let uri = topcoat::router::request::uri(cx);
    let target = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |path| path.as_str());
    super::workspace::common_screen(cx, &super::super::shell::ParsedRoute::parse(target))
}

/// Disposable Home content keeps its own live reads and browser input lifetime.
pub(super) fn region(cx: &Cx, account: i64, palette_open: Signal<bool>) -> BoxView<'_> {
    let inputs = signal(cx, String::new);
    let refresh_revision = signal(cx, || 0_usize);
    let activity_state = signal(cx, String::new);
    let content_palette = palette_open;
    let content = view! { cx =>
        <section data-native-home="" class="tc-native-home"
            (super::home_refresh::mount(cx, inputs.clone(), refresh_revision.clone()))>
            <span hidden="hidden" (super::bookmark::mount(cx))></span>
            native_home_content(account: account, browser_inputs: $(inputs.get()), refresh_revision: $(refresh_revision.get()), palette_open: content_palette, activity_state: activity_state)
        </section>
    }
    .boxed();
    home_shell::page_region(cx, content, None, "Home".to_owned())
}

#[shard("/__native_home/content")]
async fn native_home_content(
    cx: &Cx,
    account: i64,
    browser_inputs: String,
    refresh_revision: usize,
    palette_open: Signal<bool>,
    activity_state: Signal<String>,
) -> topcoat::Result<impl View> {
    let _ = refresh_revision;
    // Subscribe before reading so an edit committed during the snapshot cannot
    // be missed between the initial read and the live content lifetime.
    let events = app_context::<crate::realtime::RealtimeHub>(cx).subscribe();
    let snapshot = authorized_snapshot(cx)?;
    if snapshot.user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let connected = connected(cx);
    let content = home_live::body(
        cx,
        events,
        snapshot,
        browser_inputs,
        palette_open,
        connected,
        activity_state,
    );
    // The physical sockets retain their authority; only this body is refreshed.
    Ok(content)
}

pub(super) fn content_view<'a>(
    cx: &'a Cx,
    snapshot: &home_data::Snapshot,
    browser_inputs: &str,
    palette_open: Signal<bool>,
    connected: bool,
    activity_rate: BoxView<'a>,
) -> BoxView<'a> {
    let local = if browser_inputs.is_empty() {
        None
    } else {
        match home_local::BrowserInputs::parse(browser_inputs) {
            Ok(inputs) => Some(inputs),
            Err(error) => {
                tracing::warn!(error = %error, "invalid native Home browser inputs");
                None
            }
        }
    };
    let greeting = local
        .as_ref()
        .map(|inputs| home_local::local_greeting(inputs, Some(&snapshot.user)));
    let recents = local
        .as_ref()
        .map(home_local::stored_recents)
        .unwrap_or_default();
    let name = if snapshot.user.display_name.is_empty() {
        &snapshot.user.username
    } else {
        &snapshot.user.display_name
    };
    let (text, date, icon) = match greeting {
        Some(greeting) => (
            greeting.text,
            greeting.date_label,
            match greeting.icon {
                home_local::GreetingIcon::Moon => UiIcon::Night,
                home_local::GreetingIcon::Sunrise => UiIcon::Morning,
                home_local::GreetingIcon::Sun => UiIcon::Day,
                home_local::GreetingIcon::Sunset => UiIcon::Evening,
            },
        ),
        // The browser supplies its clock after mount. Do not invent its date.
        None => (format!("Welcome, {name}"), String::new(), UiIcon::Day),
    };
    let model = home_model::derive_home(&snapshot.projects, &snapshot.issues);
    let quick_issue_url = model
        .quick_issue_project
        .map(|project| mounted_url(cx, &format!("/{}/issues/new", project.identifier)));
    let work = home_view::active_work(cx, model);
    let rail = home_sections::right_rail_with_rate(
        cx,
        &snapshot.projects,
        &snapshot.pinned_pages,
        &snapshot.activity,
        &recents,
        activity_rate,
    );
    view! { cx =>
        <div class="tc-native-home__page" data-native-home-connected=(if connected { "true" } else { "false" })>
            <header class="tc-native-home__hero">
                <div class="tc-native-home__greeting">
                    <span class="tc-native-home__greeting-icon" aria-hidden="true">(super::icons::ui_icon(cx, icon, 20))</span>
                    <div><h1 id="native-home-greeting">(text)</h1><p id="native-home-date">(date)</p></div>
                </div>
                <div class="tc-native-home__actions">
                    if let Some(url) = quick_issue_url {
                        <a class="tc-native-home__new" href=(url)>(super::icons::ui_icon(cx, UiIcon::Add, 14)) "New issue"</a>
                    }
                    <button type="button" id="native-home-quick-jump" @click=$(|_event| palette_open.set(true))>
                        (super::icons::ui_icon(cx, UiIcon::KeyboardShortcut, 13)) "Jump to…"
                        <kbd>"⌘K"</kbd>
                    </button>
                </div>
            </header>
            <div class="tc-native-home__columns">(work)(rail)</div>
        </div>
    }.boxed()
}
