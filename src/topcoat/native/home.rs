//! Production Home. Application reads and presentation run in Rust.

use topcoat::{
    context::{Cx, app_context},
    runtime::{Signal, connected, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

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

fn authorized_snapshot(cx: &Cx) -> topcoat::Result<home_data::Snapshot> {
    super::session::read(cx, home_data::snapshot(cx))
}

pub(crate) fn screen(cx: &Cx) -> topcoat::Result<BoxView<'_>> {
    let snapshot = authorized_snapshot(cx)?;
    let account_id = snapshot.user.id;
    let account_admin = snapshot.user.is_admin;
    let initialized = signal(cx, || false);
    let inputs = signal(cx, String::new);
    let palette_open = signal(cx, || false);
    let content_palette = palette_open.clone();
    let content = view! { cx =>
        <section data-native-home="" class="tc-native-home"
            (super::browser_inputs::mount(cx, initialized, inputs.clone(), "lific_recents".into()))>
            <span hidden="hidden" (super::session::account_mount(cx, account_id, account_admin))></span>
            <span hidden="hidden" (super::bookmark::mount(cx))></span>
            native_home_content(browser_inputs: $(inputs.get()), palette_open: content_palette)
        </section>
    }
    .boxed();
    Ok(home_shell::shell_with_palette(
        cx,
        &snapshot,
        content,
        palette_open,
    ))
}

#[shard("/__native_home/content")]
async fn native_home_content(
    cx: &Cx,
    browser_inputs: String,
    palette_open: Signal<bool>,
) -> topcoat::Result<impl View> {
    let revocations = super::session::subscribe_revocations(cx);
    // Subscribe before reading so an edit committed during the snapshot cannot
    // be missed between the initial read and the live content lifetime.
    let events = app_context::<crate::realtime::RealtimeHub>(cx).subscribe();
    let snapshot = authorized_snapshot(cx)?;
    let connected = connected(cx);
    let session_lifetime = super::session::revocation_lifetime(
        cx,
        revocations,
        snapshot.user.id,
        snapshot.user.is_admin,
        connected,
    );
    let content = home_live::body(
        cx,
        events,
        snapshot,
        browser_inputs,
        palette_open,
        connected,
    );
    // Refresh only the body region. Session retirement and the palette-owning
    // shell keep their original owning scopes and existing sibling sockets.
    Ok(view! { (session_lifetime)(content) })
}

pub(super) fn content_view<'a>(
    cx: &'a Cx,
    snapshot: &home_data::Snapshot,
    browser_inputs: &str,
    palette_open: Signal<bool>,
    connected: bool,
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
                home_local::GreetingIcon::Moon => "lucide:Moon",
                home_local::GreetingIcon::Sunrise => "lucide:Sunrise",
                home_local::GreetingIcon::Sun => "lucide:Sun",
                home_local::GreetingIcon::Sunset => "lucide:Sunset",
            },
        ),
        // The browser supplies its clock after mount. Do not invent its date.
        None => (format!("Welcome, {name}"), String::new(), "lucide:Sun"),
    };
    let model = home_model::derive_home(&snapshot.projects, &snapshot.issues);
    let quick_issue_url = model
        .quick_issue_project
        .map(|project| mounted_url(cx, &format!("/{}/issues/new", project.identifier)));
    let work = home_view::active_work(cx, model);
    let rail = home_sections::right_rail(
        cx,
        &snapshot.projects,
        &snapshot.pinned_pages,
        &snapshot.activity,
        &recents,
    );
    view! { cx =>
        <div class="tc-native-home__page" data-native-home-connected=(if connected { "true" } else { "false" })>
            <header class="tc-native-home__hero">
                <div class="tc-native-home__greeting">
                    <span class="tc-native-home__greeting-icon" aria-hidden="true">(super::icons::project_icon(cx, Some(icon), 20))</span>
                    <div><h1 id="native-home-greeting">(text)</h1><p id="native-home-date">(date)</p></div>
                </div>
                <div class="tc-native-home__actions">
                    if let Some(url) = quick_issue_url {
                        <a class="tc-native-home__new" href=(url)>(super::icons::project_icon(cx, Some("lucide:Plus"), 14)) "New issue"</a>
                    }
                    <button type="button" id="native-home-quick-jump" @click=$(|_event| palette_open.set(true))>
                        (super::icons::project_icon(cx, Some("lucide:Command"), 13)) "Jump to…"
                        <kbd>"⌘K"</kbd>
                    </button>
                </div>
            </header>
            <div class="tc-native-home__columns">(work)(rail)</div>
        </div>
    }.boxed()
}
