//! Issue confirmation UI; the persistent workspace owns deferred deletion.
use super::super::icons::UiIcon;
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, StringSurrogate, expr, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

pub(crate) const STYLESHEET: &str = include_str!("delete_menu.css");

#[derive(Clone, serde::Serialize)]
pub(crate) struct Request {
    #[serde(serialize_with = "serialize_id")]
    pub(crate) account_id: i64,
    #[serde(serialize_with = "serialize_id")]
    pub(crate) issue_id: i64,
    pub(crate) identifier: String,
    /// Logical private destination, without the trusted mount prefix.
    pub(crate) list_path: String,
    pub(crate) detail_path: String,
}

// DOM transport preserves SQLite IDs before the parent hydrates typed i64 values.
fn serialize_id<S: serde::Serializer>(id: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(id)
}

pub(crate) fn toolbar<'a>(
    cx: &'a Cx,
    request: Request,
    can_edit: bool,
    project: &str,
) -> BoxView<'a> {
    let project = project.to_owned();
    view! { cx =>
        if can_edit { delete_menu(request: request, project: project) }
    }
    .boxed()
}

#[component]
async fn delete_menu(cx: &Cx, request: Request, project: String) -> topcoat::Result<impl View> {
    let menu_open = signal(cx, || false);
    let confirming = signal(cx, || false);
    let deleting = signal(cx, || false);
    let id = format!("native-issue-delete-menu-{}", request.identifier);
    let mount_id = id.clone();
    let label = format!("Delete {}?", request.identifier);
    let payload = serde_json::to_string(&request).expect("the native delete request serializes");
    let board_path = super::list_return::board_path(&project);
    let mut board_request = request;
    board_request.list_path.clone_from(&board_path);
    let board_payload =
        serde_json::to_string(&board_request).expect("the native delete request serializes");
    let click_deleting = deleting.clone();
    let click_confirming = confirming.clone();
    let click_menu_open = menu_open.clone();
    let click = expr!(|destination: StringSurrogate| {
        if !click_deleting.get() {
            click_deleting.set(true);
            let _selected_payload = if destination == board_path {
                board_payload.clone()
            } else {
                payload.clone()
            };
            // Generic DOM dispatch only. A genuine workspace owner
            // acknowledges acceptance with preventDefault(); no
            // listener means no scheduling or deletion took place.
            let accepted = raw!(
                "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-issue-delete-request', {detail:JSON.parse(${_selected_payload}.toString()), cancelable:true})))",
                false
            );
            if !accepted {
                click_deleting.set(false);
                click_confirming.set(false);
                click_menu_open.set(false);
            }
        }
    });
    let mut delete_attributes = Attributes::with_capacity(1);
    delete_attributes.insert(
        cx,
        "data-topcoat-on:click",
        super::list_return::handler(&project, &click),
    );
    Ok(view! { cx =>
        <div id=(id) class="native-issue-detail__delete" @mount=$(|_mount: Event| {
            // Restored old render signals do not reopen a fresh issue's menu.
            menu_open.set(false); confirming.set(false); deleting.set(false);
            let _dismiss = |outside: BoolSurrogate| {
                if outside { menu_open.set(false); confirming.set(false); }
            };
            raw!(
                "window.addEventListener('click', event => ${_dismiss}(cx.hydrate(!document.getElementById(${mount_id}.toString())?.contains(event.target))), {signal:cx.abortSignal})",
                ()
            );
        })>
            <button type="button" class="native-issue-detail__more" title="More actions" @click=$(|_event: Event| {
                if confirming.get() { confirming.set(false); menu_open.set(false); }
                else { menu_open.set(!menu_open.get()); }
            })>(super::super::icons::ui_icon(cx, UiIcon::MoreActions, 14))</button>
            <div class="native-issue-detail__delete-menu" :hidden=$(if menu_open.get() { confirming.get() } else { true })>
                <button type="button" class="native-issue-detail__delete-option" @click=$(|_event: Event| confirming.set(true))>
                    (super::super::icons::ui_icon(cx, UiIcon::Delete, 14))"Delete issue"
                </button>
            </div>
            <div class="native-issue-detail__delete-confirm" :hidden=$(!confirming.get())>
                <p class="native-issue-detail__delete-title">(label)</p>
                <p class="native-issue-detail__delete-body">"This can't be undone."</p>
                <div class="native-issue-detail__delete-buttons">
                    <button type="button" class="native-issue-detail__delete-run" :disabled=$(deleting.get()) (delete_attributes)>$(if deleting.get() { "Deleting..." } else { "Delete" })</button>
                    <button type="button" class="native-issue-detail__delete-cancel" @click=$(|_event: Event| {
                        confirming.set(false); menu_open.set(false);
                    })>"Cancel"</button>
                </div>
            </div>
        </div>
    })
}
