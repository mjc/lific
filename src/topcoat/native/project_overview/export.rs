//! Viewer-authorized native project ZIP export and its independent toolbar state.

use topcoat::{
    context::Cx,
    router::{Body, path_param, response::Response, route},
    runtime::{Event, signal},
    view::{BoxView, ViewExt, view},
};

use super::super::{context, session, transport};

path_param!(identifier);

#[route(GET "/__native_overview/export/{identifier}")]
async fn download(cx: &Cx) -> topcoat::Result<Response> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let identifier = path_param::<Identifier>(cx).to_owned();
    let response = session::read(
        cx,
        crate::services::export_project::project(
            context::db(cx).clone(),
            &caller.identity,
            identifier,
            None,
        )
        .await,
    )?;
    // The body retains the existing temp directory, slot and stream deadlines.
    Ok(response.map(Body::new))
}

/// The error precedes mode controls; the button follows the save-status span.
/// Both fragments share this one export owner, including Viewer documents.
pub(crate) fn toolbar_fragments<'a>(cx: &'a Cx, identifier: &str) -> (BoxView<'a>, BoxView<'a>) {
    let exporting = signal(cx, || false);
    let error = signal(cx, String::new);
    let error_message = error.clone();
    let error_view = view! { cx =>
        <span class="native-overview__export-error" data-native-overview-export-error=""
            :hidden=$(error_message.get().is_empty())>$(error_message.get())</span>
    }
    .boxed();
    let endpoint = transport::mounted_url(cx, &format!("/__native_overview/export/{identifier}"));
    let button_id = format!("native-overview-export-{identifier}");
    let _listener_id = button_id.clone();
    let button = view! { cx =>
        <button id=(button_id) class="native-overview__export toolbar-pill" type="button"
            :aria-label=$(if exporting.get() { "Exporting" } else { "Export" })
            :disabled=$(exporting.get()) @mount=$(|_mount: Event| {
                // A refreshed toolbar has no operation corresponding to an old
                // restored busy signal. Its mount owns the new click listener.
                exporting.set(false);
                let _click = || {
                    if !exporting.get() {
                        exporting.set(true);
                        error.set("".to_owned());
                        // Fetch/Blob/anchor operations are browser primitives. Rust
                        // owns duplicate prevention, state transitions and policy.
                        let _completed = |failure: topcoat::runtime::StringSurrogate| {
                            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                error.set(failure);
                                exporting.set(false);
                            }
                        };
                        raw!(r#"void (async () => {
                            try {
                                const response = await fetch(${endpoint}.toString(), {
                                    signal: cx.abortSignal,
                                    redirect: 'error'
                                });
                                if (!response.ok) return 'HTTP ' + response.status;
                                const filename = response.headers.get('content-disposition')
                                    ?.match(/filename="([^"]+)"/)?.[1] || 'download';
                                const blob = await response.blob();
                                if (cx.abortSignal.aborted) return '';
                                const url = URL.createObjectURL(blob);
                                const anchor = document.createElement('a');
                                try {
                                    anchor.href = url;
                                    anchor.download = filename;
                                    document.body.appendChild(anchor);
                                    anchor.click();
                                } finally {
                                    anchor.remove();
                                    setTimeout(() => URL.revokeObjectURL(url), 1000);
                                }
                                return '';
                            } catch (failure) {
                                return failure instanceof Error ? failure.message : String(failure);
                            }
                        })().then(failure => ${_completed}(cx.hydrate(failure)))"#, ());
                    }
                };
                raw!(
                    "document.getElementById(${_listener_id}.toString()).addEventListener('click', ${_click}, {signal:cx.abortSignal})",
                    ()
                );
            })>
            (super::super::icons::project_icon(cx, Some("lucide:Download"), 14))
            <span class="native-overview__export-label" :hidden=$(exporting.get())>"Export"</span>
            <span class="native-overview__export-label" :hidden=$(!exporting.get())>"Exporting..."</span>
        </button>
    }.boxed();
    (error_view, button)
}
