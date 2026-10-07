//! Both physical layouts render the same bounded, authorized recent rows.
use super::{
    recents_actions,
    recents_state::{self, Handles, Signals},
    view::Layout,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard},
    view::{BoxView, View, ViewExt, view},
};

#[shard("/__native_sidebar/recents_driver")]
pub(super) async fn driver(
    cx: &Cx,
    account: i64,
    path: String,
    handles: Handles,
    catalog: Signal<String>,
) -> topcoat::Result<impl View> {
    let state = Signals::from_handles(account, handles);
    let mount = recents_state::refresh(cx, &state, catalog, path, "mount", false);
    Ok(view! {
        cx =>
        <span hidden="hidden" data-native-recents-driver="" (mount)></span>
    }
    .boxed())
}

pub(super) fn slot<'a>(
    cx: &'a Cx,
    state: &Signals,
    project: &super::model::Project,
    path: &str,
    layout: Layout,
) -> BoxView<'a> {
    let Some(section) = super::recents_model::Section::for_path(&project.identifier, path) else {
        return view! { cx => <span></span> }.boxed();
    };
    let account = state.account;
    let project_id = project.id;
    let expected_label = format!("Recent {}", section.as_str());
    let rows = state.rows.clone();
    let label = state.label.clone();
    let visible = state.visible.clone();
    let selected = state.project.clone();
    let open = state.open.clone();
    let loading = state.loading.clone();
    let error = state.error.clone();
    let status = state.status.clone();
    let focus = state.focus.clone();
    let suffix = if matches!(layout, Layout::Desktop) {
        "desktop"
    } else {
        "phone"
    };
    let content = format!("recent-{project_id}-{suffix}");
    let toggle_id = format!("recent-toggle-{project_id}-{suffix}");
    let toggle = recents_state::disclosure(cx, state);
    let path = path.to_owned();
    let root = format!("native-recents-{project_id}-{suffix}");
    view! {
        cx =>
        <section
            id=(root)
            class="sidebar-recents"
            data-topcoat-recents=""
            data-native-recents-project=(project_id.to_string())
            :hidden=$(if visible.get() {
                if selected.get() == project_id {
                    label.get() != expected_label
                } else {
                    true
                }
            } else {
                true
            })
        >
            <button
                id=(toggle_id.clone())
                type="button"
                class="recent-toggle"
                data-recents-toggle=""
                :aria-expanded=$(if open.get() { "true" } else { "false" })
                aria-controls=(content.clone())
                (toggle)
            >
                $(label.get())
            </button>
            <div
                id=(content)
                data-recents-content=""
                :hidden=$(!open.get())
                :aria-busy=$(if loading.get() { "true" } else { "false" })
            >
                <span data-recents-status="" role="status" aria-live="polite">
                    $(status.get())
                </span>
                <p data-recents-error="" role="alert" :hidden=$(error.get().is_empty())>
                    $(error.get())
                </p>
                <div data-recents-list="">
                    native_rows(
                        account: account,
                        wire: $(rows.get()),
                        project_id: project_id,
                        path: path,
                        focus: focus,
                        toggle_id: toggle_id
                    )
                </div>
            </div>
        </section>
    }
    .boxed()
}

use rows_shard::native_rows;
#[allow(
    clippy::too_many_arguments,
    reason = "One recents shard expands an implicit context plus separate reactive row inputs"
)]
mod rows_shard {
    use super::*;
    use crate::server::topcoat_frontend::native::navigation;

    #[shard("/__native_sidebar/recents_rows")]
    pub(super) async fn native_rows(
        cx: &Cx,
        account: i64,
        wire: String,
        project_id: i64,
        path: String,
        focus: Signal<String>,
        toggle_id: String,
    ) -> topcoat::Result<impl View> {
        let empty = || view! { cx => <span></span> }.boxed();
        if wire.len() > 256 * 1024 {
            return Ok(empty());
        }
        let parsed: recents_actions::Rows = match serde_json::from_str(&wire) {
            Ok(rows) => rows,
            Err(_) => return Ok(empty()),
        };
        if parsed
            .1
            .as_ref()
            .is_none_or(|project| project.0 != project_id)
        {
            return Ok(empty());
        }
        let rows = recents_actions::rows(cx, account, &wire).unwrap_or_default();
        let mut links = Vec::new();
        for row in rows {
            let key = row.href.clone();
            let href = row.href.clone();
            let active = path == row.href;
            let title = row.accessible_name();
            let number = row
                .identifier
                .as_deref()
                .and_then(|identifier| identifier.rsplit('-').next())
                .map(|sequence| format!("#{sequence}"));
            let identifier = row.identifier.unwrap_or_default();
            links.push(
                view! {
                    cx =>
                    <a
                        class="sidebar-destination recent-link"
                        (navigation::attrs(cx, &href))
                        data-recents-href=(key)
                        data-recents-identifier=(identifier)
                        aria-label=(title.clone())
                        title=(title)
                        aria-current=(active.then_some("page"))
                    >
                        if let Some(number) = number {
                            <span class="recent-identifier" aria-hidden="true">
                                (number)
                            </span>
                        }
                        <span data-recents-label="" class="recent-label">
                            (row.label.clone())
                        </span>
                        <span class="focus-title" aria-hidden="true">(row.label)</span>
                    </a>
                }
                .boxed(),
            );
        }
        let mounted = expr!(|_event: Event| {
            let _wire = focus.get();
            if !_wire.is_empty() {
                let _target = raw!(
                    "cx.hydrate(JSON.parse(${_wire}.toString())[0])",
                    String::new()
                );
                let _source = raw!(
                    "cx.hydrate(JSON.parse(${_wire}.toString())[1])",
                    String::new()
                );
                if !_target.is_empty() {
                    let _done = || {
                        focus.set("".to_owned());
                    };
                    raw!(
                        "requestAnimationFrame(()=>{if(cx.abortSignal.aborted)return;const t=document.getElementById(${toggle_id}.toString());const r=t?.closest('[data-topcoat-recents]');if(!r||!r.getClientRects().length)return;const active=document.activeElement;const actual=active?.closest('[data-recents-list] a')?.getAttribute('data-recents-href')??'';if(active&&active!==document.body&&active!==document.documentElement&&actual!==${_source}.toString()){${_done}();return;}const wanted=${_target}.toString();const n=wanted==='disclosure'?t:[...r.querySelectorAll('a[data-recents-href]')].find(a=>a.getAttribute('data-recents-href')===wanted);if(n){n.focus();${_done}();}});",
                        ()
                    );
                }
            }
        });
        Ok(view! {
            cx =>
            <div data-native-recents-rows="" @mount=(mounted)>
                for link in links {
                    (link)
                }
            </div>
        }
        .boxed())
    }
}
