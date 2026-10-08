//! Shared Module icon picker workflows.
use super::super::{browser, project_create};
use super::detail::update_module;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(super) fn list_picker(cx: &Cx, selected: Signal<String>) -> BoxView<'_> {
    project_create::icon_picker(cx, selected, Attributes::with_capacity(0))
}

pub(super) fn detail_picker<'a>(
    cx: &'a Cx,
    owner: &Cx,
    account: i64,
    project_id: i64,
    module_id: i64,
    selected: Signal<String>,
) -> BoxView<'a> {
    let browser = browser::bindings();
    let canonical = signal(owner, || selected.read_untracked().clone());
    let busy = signal(owner, || false);
    let error = signal(owner, String::new);
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let failed_selected = selected.clone();
    let failed_canonical = canonical.clone();
    let handler = expr!(|_event: Event| {
        if browser.is_disposed() {
            return;
        }
        if busy.get() {
            selected.set(canonical.get());
            return;
        }
        let next = selected.get();
        if next == canonical.get() {
            return;
        }
        // The picker updates its selection before emitting the change event.
        // Restore the last saved icon while the durable owner saves the new one.
        selected.set(canonical.get());
        busy.set(true);
        error.set("".to_owned());
        let _failed = || {
            if !browser.is_disposed() {
                failed_busy.set(false);
                failed_selected.set(failed_canonical.get());
                failed_error.set(
                    "Couldn't save module: Couldn't reach the server. Check your connection and try again."
                        .to_owned(),
                );
            }
        };
        let _save = async || {
            if browser.is_disposed() {
                return;
            }
            update_module(
                account,
                project_id,
                module_id,
                "emoji".to_owned(),
                next.clone(),
            )
            .await;
            if !browser.is_disposed() {
                busy.set(false);
                canonical.set(next.clone());
                selected.set(next.clone());
            }
        };
        browser.microtask(|| {
            if !browser.is_disposed() {
                raw!(
                    "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                    ()
                );
            }
        });
    });
    let mut changed = Attributes::with_capacity(1);
    changed.insert(
        owner,
        "data-topcoat-on:native-project-icon-change",
        handler.into_evaluated_and_js().1,
    );
    let picker = project_create::icon_picker(cx, selected.clone(), changed);
    view! {
        cx =>
        <div class="flex items-center gap-2" data-native-module-icon-picker="">
            (picker)
            <span
                class="text-caption text-[var(--text-muted)]"
                role="status"
                :hidden=$(!busy.get())
            >
                "Saving…"
            </span>
            <p
                class="text-caption text-[var(--error)] m-0"
                role="alert"
                :hidden=$(error.get().is_empty())
            >
                $(error.get())
            </p>
        </div>
    }
    .boxed()
}
