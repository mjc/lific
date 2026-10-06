//! Inline identity editing and per-user filing, authored in Rust.
use super::super::icons;
use super::super::icons::UiIcon;
use super::actions;
use crate::db::models::{Project, ProjectGroup};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct Controls {
    account: i64,
    project: i64,
    pub(super) name: Signal<String>,
    pub(super) description: Signal<String>,
    emoji: Signal<String>,
    stored_emoji: Signal<String>,
    name_draft: Signal<String>,
    description_draft: Signal<String>,
    name_editing: Signal<bool>,
    description_editing: Signal<bool>,
    busy: Signal<bool>,
    pub(super) saved: Signal<bool>,
    saved_revision: Signal<usize>,
    pub(super) error: Signal<String>,
}
impl Controls {
    pub(super) fn new(cx: &Cx, account: i64, project: &Project) -> Self {
        Self {
            account,
            project: project.id,
            name: signal(cx, || project.name.clone()),
            description: signal(cx, || project.description.clone()),
            emoji: signal(cx, || project.emoji.clone().unwrap_or_default()),
            stored_emoji: signal(cx, || project.emoji.clone().unwrap_or_default()),
            name_draft: signal(cx, || project.name.clone()),
            description_draft: signal(cx, || project.description.clone()),
            name_editing: signal(cx, || false),
            description_editing: signal(cx, || false),
            busy: signal(cx, || false),
            saved: signal(cx, || false),
            saved_revision: signal(cx, || 0_usize),
            error: signal(cx, String::new),
        }
    }
}
fn save(cx: &Cx, controls: &Controls, field: &str, event_name: &str) -> Attributes {
    let controls = controls.clone();
    let name = controls.name.clone();
    let description = controls.description.clone();
    let name_draft = controls.name_draft.clone();
    let description_draft = controls.description_draft.clone();
    let name_editing = controls.name_editing.clone();
    let description_editing = controls.description_editing.clone();
    let field_busy = controls.busy.clone();
    let saved = controls.saved.clone();
    let saved_revision = controls.saved_revision.clone();
    let error = controls.error.clone();
    let field = field.to_owned();
    let keyboard = event_name == "keydown";
    let account = controls.account;
    let project = controls.project;
    let editing = if field == "name" {
        name_editing
    } else {
        description_editing
    };
    let draft = if field == "name" {
        name_draft
    } else {
        description_draft
    };
    let failed_busy = field_busy.clone();
    let failed_error = error.clone();
    let save_field = actions::save_field;
    let handler = expr!(async |event: Event| {
        if if keyboard {
            event.key == "Escape"
        } else {
            false
        } {
            event.prevent_default();
            editing.set(false);
        } else {
            let submit = if keyboard {
                if event.key == "Enter" {
                    if field == "name" {
                        true
                    } else if event.ctrl_key {
                        true
                    } else {
                        event.meta_key
                    }
                } else {
                    false
                }
            } else {
                true
            };
            if if submit {
                if editing.get() {
                    !field_busy.get()
                } else {
                    false
                }
            } else {
                false
            } {
                if keyboard {
                    event.prevent_default();
                }
                // Hide first so a blur induced by Enter cannot submit twice.
                editing.set(false);
                let value = draft.get();
                field_busy.set(true);
                error.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_error.set("Couldn't save changes. Try again.".to_owned());
                };
                let _save = async || {
                    let outcome = save_field(account, project, field, value).await;
                    field_busy.set(false);
                    if outcome.0.is_ok() {
                        name.set(outcome.1);
                        description.set(outcome.2);
                        if outcome.0.unwrap() == "saved" {
                            saved.set(true);
                            saved_revision.increment();
                            let expected = saved_revision.get();
                            let _clear_saved = || {
                                if saved_revision.get() == expected {
                                    saved.set(false);
                                }
                            };
                            raw!(
                                "setTimeout(()=>{if(!cx.abortSignal.aborted) ${_clear_saved}()},2000);",
                                ()
                            );
                        }
                    } else {
                        error.set(outcome.0.unwrap_err());
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        format!("data-topcoat-on:{event_name}"),
        handler.into_evaluated_and_js().1,
    );
    attrs
}

pub(super) fn name<'a>(cx: &'a Cx, controls: &Controls, can_manage: bool) -> BoxView<'a> {
    let controls = controls.clone();
    let name = controls.name.clone();
    let name_draft = controls.name_draft.clone();
    let name_editing = controls.name_editing.clone();
    let input_id = format!("native-overview-name-{}", controls.project);
    let focus_input = input_id.clone();
    let blur = save(cx, &controls, "name", "blur");
    let keydown = save(cx, &controls, "name", "keydown");
    view! { cx =>
        if can_manage {
            <button type="button" class="native-overview__name" :hidden=$(name_editing.get()) @click=$(|_event: Event| {
                name_draft.set(name.get()); name_editing.set(true);
                raw!("requestAnimationFrame(()=>document.getElementById(${focus_input}.toString())?.focus());", ());
            })>$(name.get())(icons::ui_icon(cx,UiIcon::Edit,14))</button>
            <input id=(input_id) aria-label="Project name" class="native-overview__name-input" :hidden=$(!name_editing.get()) :value=$(name_draft.get())
                @input=$(|event: Event| name_draft.set(event.target.value))
                (blur) (keydown)>
        } else { <h1 class="native-overview__name">$(name.get())</h1> }
    }.boxed()
}
pub(super) fn description<'a>(cx: &'a Cx, controls: &Controls, can_manage: bool) -> BoxView<'a> {
    let controls = controls.clone();
    let description = controls.description.clone();
    let description_draft = controls.description_draft.clone();
    let description_editing = controls.description_editing.clone();
    let input_id = format!("native-overview-description-{}", controls.project);
    let focus_input = input_id.clone();
    let blur = save(cx, &controls, "description", "blur");
    let keydown = save(cx, &controls, "description", "keydown");
    view! { cx =>
        if can_manage {
            <button type="button" class="native-overview__description-button" :hidden=$(description_editing.get()) @click=$(|_event: Event| {
                description_draft.set(description.get()); description_editing.set(true);
                raw!("requestAnimationFrame(()=>document.getElementById(${focus_input}.toString())?.focus());", ());
            })>
                <span :hidden=$(description.get().is_empty())>$(description.get())</span>
                <span class="native-overview__description-empty" :hidden=$(!description.get().is_empty())>"Add a description…"</span>
                " "(icons::ui_icon(cx,UiIcon::Edit,12))
            </button>
            <textarea id=(input_id) rows="2" aria-label="Project description" placeholder="Describe this project…" class="native-overview__description-input" :hidden=$(!description_editing.get()) :value=$(description_draft.get())
                @input=$(|event: Event| description_draft.set(event.target.value))
                (blur) (keydown)></textarea>
        } else { <p class="native-overview__description" :hidden=$(description.get().is_empty())>$(description.get())</p> }
    }.boxed()
}

pub(super) fn copy_identifier<'a>(cx: &'a Cx, identifier: &str) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let copied = signal(cx, || false);
    let completed_copied = copied.clone();
    let failed_copied = copied.clone();
    let clipboard_identifier = identifier.clone();
    view! { cx => <button type="button" class="native-overview__identifier" aria-label=(identifier.clone()) @click=$(|_event: Event| {
        let _completed = || { completed_copied.set(true); let _reset = || completed_copied.set(false); raw!("setTimeout(()=>{if(!cx.abortSignal.aborted) ${_reset}()},1500);", ()); };
        let _failed = || failed_copied.set(false);
        raw!("navigator.clipboard.writeText(${clipboard_identifier}.toString()).then(()=>${_completed}(),()=>${_failed}());", ());
    })>(identifier)<span :hidden=$(copied.get())>(icons::ui_icon(cx,UiIcon::Copy,11))</span><span :hidden=$(!copied.get())>(icons::ui_icon(cx,UiIcon::Copied,11))</span></button> }.boxed()
}

pub(super) fn group<'a>(
    cx: &'a Cx,
    controls: &Controls,
    project: &Project,
    groups: &[ProjectGroup],
) -> BoxView<'a> {
    let controls = controls.clone();
    let saved = controls.saved.clone();
    let saved_revision = controls.saved_revision.clone();
    let error = controls.error.clone();
    let initial = groups
        .iter()
        .find(|group| group.project_ids.contains(&project.id))
        .map(|group| group.id);
    let selected = signal(cx, || initial.map_or(String::new(), |id| id.to_string()));
    let stored = signal(cx, || initial.map_or(String::new(), |id| id.to_string()));
    let busy = signal(cx, || false);
    let account = controls.account;
    let project_id = project.id;
    let rows = groups
        .iter()
        .map(|group| (group.id.to_string(), group.name.clone()))
        .collect::<Vec<_>>();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let failed_selected = selected.clone();
    let failed_stored = stored.clone();
    let assign_group = actions::assign_group;
    view! { cx => <section class="native-overview__group"><div class="native-overview__group-row"><div><p>"Sidebar group"</p><p>"Where this project sits in your sidebar. Only you see it."</p></div>
        <select aria-label="Sidebar group" :value=$(selected.get()) :disabled=$(busy.get()) @change=$(async |event: Event| {
            let value = event.target.value;
            selected.set(value.clone()); busy.set(true); error.set("".to_owned());
            let _failed = || { failed_busy.set(false); failed_selected.set(failed_stored.get()); failed_error.set("Couldn't save changes. Try again.".to_owned()); };
            let _save = async || {
                let outcome = assign_group(account,project_id,value.clone()).await;
                busy.set(false);
                if outcome.is_ok() { stored.set(value); saved.set(true); saved_revision.increment(); let expected = saved_revision.get(); let _clear = || { if saved_revision.get() == expected { saved.set(false); } }; raw!("setTimeout(()=>{if(!cx.abortSignal.aborted) ${_clear}()},2000);", ()); }
                else { selected.set(stored.get()); error.set(outcome.unwrap_err()); }
            };
            raw!("Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());", ());
        })><option value="">"No group"</option>for (value, label) in rows { <option value=(value)>(label)</option> }</select></div>
    </section> }.boxed()
}

// Shared ProjectForm picker emits only a generic selection event. Rust saves,
// rolls back on refusal, and owns the saved indicator exactly as inline fields.
pub(super) fn icon<'a>(cx: &'a Cx, controls: &Controls) -> BoxView<'a> {
    let account = controls.account;
    let project = controls.project;
    let emoji = controls.emoji.clone();
    let stored = controls.stored_emoji.clone();
    let busy = controls.busy.clone();
    let saved = controls.saved.clone();
    let revision = controls.saved_revision.clone();
    let error = controls.error.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let failed_emoji = emoji.clone();
    let failed_stored = stored.clone();
    let save_field = actions::save_field;
    let handler = expr!(|_event: Event| {
        if if !busy.get() {
            emoji.get() != stored.get()
        } else {
            false
        } {
            let value = emoji.get();
            busy.set(true);
            error.set("".to_owned());
            let _failed = || {
                failed_busy.set(false);
                failed_emoji.set(failed_stored.get());
                failed_error.set("Couldn't save changes. Try again.".to_owned());
            };
            let _run = async || {
                let result = save_field(account, project, "emoji".to_owned(), value).await;
                busy.set(false);
                if result.0.is_ok() {
                    emoji.set(result.3.clone());
                    stored.set(result.3);
                    if result.0.unwrap() == "saved" {
                        saved.set(true);
                        revision.increment();
                        let expected = revision.get();
                        let _clear = || {
                            if revision.get() == expected {
                                saved.set(false);
                            }
                        };
                        raw!(
                            "setTimeout(()=>{if(!cx.abortSignal.aborted) ${_clear}()},2000);",
                            ()
                        );
                    }
                } else {
                    emoji.set(stored.get());
                    error.set(result.0.unwrap_err());
                }
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut changed = Attributes::with_capacity(1);
    changed.insert(
        cx,
        "data-topcoat-on:native-project-icon-change",
        handler.into_evaluated_and_js().1,
    );
    super::super::project_create::icon_picker(cx, controls.emoji.clone(), changed)
}
