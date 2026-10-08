//! Project-scoped PageDetail label display and attachment controls.

use super::super::{browser, context, label_chip, label_editor, session};
use super::editor_state::EditorState;
use super::labels_action::{Reply, ReplyValue, Request};
use crate::error::LificError;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) fn detail<'a>(
    cx: &'a Cx,
    account_id: i64,
    page_id: i64,
    identifier: String,
    state: &EditorState,
) -> BoxView<'a> {
    let open = signal(cx, || false);
    let revision = signal(cx, || 0_usize);
    let mount = mount_attributes(
        cx,
        account_id,
        page_id,
        state,
        open.clone(),
        revision.clone(),
    );
    let labels = state.labels.clone();
    let opened = open.clone();
    let busy = state.busy.clone();
    let page_state = (opened.clone(), busy, revision.clone());
    view! {
        cx =>
        <section class="relative space-y-2" data-native-page-labels="" (mount)>
            native_page_label_controls(
                account_id: account_id,
                page_id: page_id,
                identifier: identifier,
                attached: $(labels.get()),
                catalog_revision: $(revision.get()),
                state: page_state.clone()
            )
        </section>
    }
    .boxed()
}

fn mount_attributes(
    cx: &Cx,
    account_id: i64,
    page_id: i64,
    state: &EditorState,
    open: Signal<bool>,
    revision: Signal<usize>,
) -> Attributes {
    let title = state.title.clone();
    let body = state.body.clone();
    let title_draft = state.title_draft.clone();
    let body_draft = state.body_draft.clone();
    let seq = state.seq.clone();
    let status = state.status.clone();
    let pinned = state.pinned.clone();
    let labels = state.labels.clone();
    let busy = state.busy.clone();
    let opened = open.clone();
    let revision_changed = revision.clone();
    let browser = browser::bindings();
    let mount = expr!(|_event: Event| {
        let _applied = |event: Event| {
            let reply: ReplyValue = raw!(
                "${event}.inner.detail",
                Reply {
                    status: Err("".to_owned()),
                    account_id: 0_i64,
                    page_id: 0_i64,
                    canonical: None,
                }
            );
            if reply.account_id == account_id {
                if reply.page_id == page_id {
                    busy.set(false);
                    if reply.status.is_ok() {
                        if reply.canonical.is_some() {
                            let canonical = reply.canonical.unwrap();
                            let title_was_clean = title_draft.get() == title.get();
                            let body_was_clean = body_draft.get() == body.get();
                            title.set(canonical.title.clone());
                            body.set(canonical.content.clone());
                            if title_was_clean {
                                title_draft.set(canonical.title.clone());
                            }
                            if body_was_clean {
                                body_draft.set(canonical.content.clone());
                            }
                            seq.set(canonical.seq);
                            status.set(canonical.page_status);
                            pinned.set(canonical.pinned);
                            labels.set(canonical.labels);
                            revision_changed.increment();
                        }
                    }
                }
            }
        };
        let _outside = |event: Event| {
            let inside = raw!(
                "cx.hydrate(Boolean(${event}.inner.target?.closest('[data-native-page-labels]')))",
                false
            );
            if !inside {
                opened.set(false);
            }
        };
        browser.window_listener("lific:native-page-label-applied".to_owned(), _applied);
        browser.window_listener("click".to_owned(), _outside);
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
    attributes
}

type ControlsState = (Signal<bool>, Signal<bool>, Signal<usize>);

#[shard("/__native_pages/label_controls")]
async fn native_page_label_controls(
    cx: &Cx,
    account_id: i64,
    page_id: i64,
    identifier: String,
    attached: Vec<String>,
    catalog_revision: usize,
    state: ControlsState,
) -> topcoat::Result<impl topcoat::view::View> {
    let (open_signal, busy, revision) = state;
    let _catalog_revision = catalog_revision;
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account_id {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let page = session::read(
        cx,
        crate::services::pages::get(context::db(cx), &caller.identity, page_id),
    )?;
    if page.identifier != identifier {
        return session::read(cx, Err(LificError::NotFound("Page not found".into())));
    }
    let Some(project_id) = page.project_id else {
        return Err(topcoat::router::error::not_found().into());
    };
    let can_edit = match crate::services::pages::require_page_role(
        context::db(cx),
        &caller.identity,
        Some(project_id),
        crate::db::models::Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(LificError::Forbidden(_)) => false,
        Err(error) => return session::read(cx, Err(error)),
    };
    let structure = session::read(
        cx,
        crate::services::pages::project_structure(context::db(cx), &caller.identity, project_id),
    )?;
    let catalog = structure
        .labels
        .into_iter()
        .map(|label| (label.name, label.color))
        .collect::<Vec<_>>();
    let chips = attached
        .iter()
        .map(|name| {
            let color = catalog
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, color)| color.as_str());
            let action = if can_edit {
                let request = Request {
                    account_id,
                    page_id,
                    identifier: identifier.clone(),
                    label: name.clone(),
                    attach: false,
                };
                Some(request_attributes(
                    cx,
                    request,
                    revision.clone(),
                    busy.clone(),
                ))
            } else {
                None
            };
            let remove = action.map(|attrs| label_editor::remove_button(cx, name.clone(), attrs));
            label_chip::render_with_action(cx, name.clone(), color, remove)
        })
        .collect::<Vec<_>>();
    let empty = if attached.is_empty() && !can_edit {
        Some(
            view! {
                cx =>
                <span class="italic text-body-sm text-[var(--text-muted)]">
                    "No labels"
                </span>
            }
            .boxed(),
        )
    } else {
        None
    };
    let add = if can_edit {
        let mut attrs = Attributes::with_capacity(2);
        let open_handler = {
            let next_open = open_signal.clone();
            let browser = browser::bindings();
            expr!(|event: Event| {
                if !browser.is_disposed() {
                    event.prevent_default();
                    event.stop_propagation();
                    next_open.set(!next_open.get());
                }
            })
        };
        attrs.insert(
            cx,
            "data-topcoat-on:click",
            open_handler.into_evaluated_and_js().1,
        );
        let disabled = busy.clone();
        let disabled_handler = expr!(|_event: Event| disabled.get());
        attrs.insert(
            cx,
            "data-topcoat-bind:disabled",
            disabled_handler.into_evaluated_and_js().1,
        );
        Some(label_editor::add_button(cx, attrs))
    } else {
        None
    };
    let strip = label_editor::strip(cx, chips, empty, add);
    let popover = if can_edit {
        let options = catalog
            .iter()
            .map(|(name, color)| {
                let selected = attached.iter().any(|selected| selected == name);
                let attach = !selected;
                let request = Request {
                    account_id,
                    page_id,
                    identifier: identifier.clone(),
                    label: name.clone(),
                    attach,
                };
                let attrs = request_attributes(cx, request, revision.clone(), busy.clone());
                label_editor::option(cx, name.clone(), Some(color), selected, attrs)
            })
            .collect::<Vec<_>>();
        let children = if options.is_empty() {
            view! {
                cx =>
                <p class="px-3 py-2 text-body-sm text-[var(--text-faint)]">
                    "No labels defined in this project."
                </p>
            }
            .boxed()
        } else {
            view! {
                cx =>
                <div
                    role="listbox"
                    aria-label="Project labels"
                    class="max-h-[220px] overflow-y-auto"
                >
                    for option in options {
                        (option)
                    }
                </div>
            }
            .boxed()
        };
        let attrs = topcoat::view::attributes! {
            cx =>
            :hidden=$(!open_signal.get())
            data-native-page-label-picker=""
            @click=$(|event: Event| event.stop_propagation())
            @keydown=$(|event: Event| event.stop_propagation())
        };
        Some(label_editor::popover(cx, "w-[200px]", attrs, children))
    } else {
        None
    };
    Ok(view! {
        cx =>
        (strip)
        if let Some(popover) = popover {
            (popover)
        }
    })
}

fn request_attributes(
    cx: &Cx,
    request: Request,
    revision: Signal<usize>,
    busy: Signal<bool>,
) -> Attributes {
    let expected_revision = revision.get_untracked();
    let browser = browser::bindings();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            if !busy.get() {
                if revision.get() == expected_revision {
                    busy.set(true);
                    let accepted = raw!(
                        "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-page-label-request',{detail:${request},cancelable:true})))",
                        false
                    );
                    if !accepted {
                        busy.set(false);
                    }
                }
            }
        }
    });
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    let disabled = busy.clone();
    let disabled_handler = expr!(|_event: Event| disabled.get());
    attrs.insert(
        cx,
        "data-topcoat-bind:disabled",
        disabled_handler.into_evaluated_and_js().1,
    );
    attrs
}
