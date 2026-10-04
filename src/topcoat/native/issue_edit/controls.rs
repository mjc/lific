//! Rust-authored editor controls. Rich markdown and collaboration are separate.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, procedure, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

use super::{
    actions::{self, SaveOutcome, Snapshot},
    model::Field,
};

pub(crate) const STYLESHEET: &str = include_str!("controls.css");

type WireOutcome = (
    Result<String, String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn wire(outcome: SaveOutcome) -> WireOutcome {
    let (result, snapshot) = match outcome {
        SaveOutcome::Saved(snapshot) => (Ok("saved".into()), Some(snapshot)),
        SaveOutcome::Unchanged(snapshot) => (Ok("unchanged".into()), Some(snapshot)),
        SaveOutcome::Conflict(snapshot) => (Err("conflict".into()), Some(snapshot)),
        SaveOutcome::Reauth => (Err("reauth".into()), None),
        SaveOutcome::Forbidden => (Err("forbidden".into()), None),
        SaveOutcome::Invalid(message) => (Err(message), None),
    };
    match snapshot {
        Some(snapshot) => (
            result,
            Some(snapshot.seq),
            Some(snapshot.title),
            Some(snapshot.description),
            Some(snapshot.status.as_str().into()),
            Some(snapshot.priority.as_str().into()),
        ),
        None => (result, None, None, None, None, None),
    }
}

#[procedure("/__native_issue_edit/save/title")]
async fn save_title(
    cx: &Cx,
    identifier: String,
    value: String,
    observed_seq: i64,
) -> topcoat::Result<WireOutcome> {
    actions::save(cx, &identifier, Field::Title, &value, observed_seq)
        .await
        .map(wire)
}

#[procedure("/__native_issue_edit/save/description")]
async fn save_description(
    cx: &Cx,
    identifier: String,
    value: String,
    observed_seq: i64,
) -> topcoat::Result<WireOutcome> {
    actions::save(cx, &identifier, Field::Description, &value, observed_seq)
        .await
        .map(wire)
}

#[procedure("/__native_issue_edit/save/status")]
async fn save_status(
    cx: &Cx,
    identifier: String,
    value: String,
    observed_seq: i64,
) -> topcoat::Result<WireOutcome> {
    actions::save(cx, &identifier, Field::Status, &value, observed_seq)
        .await
        .map(wire)
}

#[procedure("/__native_issue_edit/save/priority")]
async fn save_priority(
    cx: &Cx,
    identifier: String,
    value: String,
    observed_seq: i64,
) -> topcoat::Result<WireOutcome> {
    actions::save(cx, &identifier, Field::Priority, &value, observed_seq)
        .await
        .map(wire)
}

struct Controls {
    identifier: String,
    seq: Signal<i64>,
    title: Signal<String>,
    description: Signal<String>,
    status: Signal<String>,
    priority: Signal<String>,
    title_draft: Signal<String>,
    description_draft: Signal<String>,
    title_revision: Signal<i64>,
    description_revision: Signal<i64>,
    title_editing: Signal<bool>,
    description_editing: Signal<bool>,
    busy: Signal<bool>,
    message: Signal<String>,
}

impl Controls {
    fn new(cx: &Cx, snapshot: &Snapshot) -> Self {
        Self {
            identifier: snapshot.identifier.clone(),
            seq: signal(cx, || snapshot.seq),
            title: signal(cx, || snapshot.title.clone()),
            description: signal(cx, || snapshot.description.clone()),
            status: signal(cx, || snapshot.status.as_str().to_owned()),
            priority: signal(cx, || snapshot.priority.as_str().to_owned()),
            title_draft: signal(cx, || snapshot.title.clone()),
            description_draft: signal(cx, || snapshot.description.clone()),
            title_revision: signal(cx, || 0i64),
            description_revision: signal(cx, || 0i64),
            title_editing: signal(cx, || false),
            description_editing: signal(cx, || false),
            busy: signal(cx, || false),
            message: signal(cx, String::new),
        }
    }
}

/// The same Rust callback owns every field's sequence and completion decisions.
fn save_attributes(
    cx: &Cx,
    controls: &Controls,
    field: Field,
    event_name: &str,
    selected: Option<String>,
) -> Attributes {
    let identifier = controls.identifier.clone();
    let field = match field {
        Field::Title => "title",
        Field::Description => "description",
        Field::Status => "status",
        Field::Priority => "priority",
    }
    .to_owned();
    let keyboard = event_name == "keydown";
    let seq = controls.seq.clone();
    let title = controls.title.clone();
    let description = controls.description.clone();
    let status = controls.status.clone();
    let priority = controls.priority.clone();
    let title_draft = controls.title_draft.clone();
    let description_draft = controls.description_draft.clone();
    let title_revision = controls.title_revision.clone();
    let description_revision = controls.description_revision.clone();
    let title_editing = controls.title_editing.clone();
    let description_editing = controls.description_editing.clone();
    let busy = controls.busy.clone();
    let message = controls.message.clone();
    let title_input = format!("native-issue-title-input-{}", controls.identifier);
    let body_input = format!("native-issue-body-input-{}", controls.identifier);
    let title_button = format!("native-issue-title-{}", controls.identifier);
    let body_button = format!("native-issue-body-edit-{}", controls.identifier);
    let login = super::super::transport::mounted_url(cx, "/login");
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let failed_field = field.clone();
    let failed_title_revision = title_revision.clone();
    let failed_body_revision = description_revision.clone();
    let failed_title_editing = title_editing.clone();
    let failed_body_editing = description_editing.clone();
    let failed_title_input = title_input.clone();
    let failed_body_input = body_input.clone();
    let handler = expr!(async |event: Event| {
        let cancel = if keyboard {
            event.key == "Escape"
        } else {
            false
        };
        if cancel {
            event.prevent_default();
            if field == "title" {
                title_editing.set(false);
                title_draft.set(title.get());
                title_revision.increment();
                raw!(
                    "requestAnimationFrame(() => document.getElementById(${title_button}.toString())?.focus())",
                    ()
                );
            } else {
                description_editing.set(false);
                description_draft.set(description.get());
                description_revision.increment();
                raw!(
                    "requestAnimationFrame(() => document.getElementById(${body_button}.toString())?.focus())",
                    ()
                );
            }
        } else {
            let submit = if keyboard {
                if event.key == "Enter" {
                    field == "title"
                } else {
                    if event.key == "s" {
                        if event.ctrl_key { true } else { event.meta_key }
                    } else {
                        false
                    }
                }
            } else {
                true
            };
            let ready = if busy.get() {
                false
            } else {
                if field == "title" {
                    title_editing.get()
                } else {
                    if field == "description" {
                        description_editing.get()
                    } else {
                        true
                    }
                }
            };
            if submit {
                if ready {
                    if keyboard {
                        event.prevent_default();
                    }
                    if field == "title" {
                        // Enter hides the input and fires blur; that blur must not save again.
                        title_editing.set(false);
                    }
                    let before_title = title.get();
                    let before_description = description.get();
                    let sent_title_revision = title_revision.get();
                    let sent_description_revision = description_revision.get();
                    let observed_seq = seq.get();
                    let value = if field == "title" {
                        title_draft.get()
                    } else {
                        if field == "description" {
                            description_draft.get()
                        } else {
                            selected.unwrap()
                        }
                    };
                    let changed = if field == "title" {
                        let trimmed = value.trim().to_owned();
                        if trimmed.is_empty() {
                            false
                        } else {
                            trimmed != before_title
                        }
                    } else {
                        true
                    };
                    if changed {
                        busy.set(true);
                        message.set("".to_owned());
                        let _failed = || {
                            failed_busy.set(false);
                            failed_message.set(
                                "Unable to save. Your draft is still here. Try again.".to_owned(),
                            );
                            if failed_field == "title" {
                                if failed_title_revision.get() == sent_title_revision {
                                    failed_title_editing.set(true);
                                    raw!(
                                        "requestAnimationFrame(() => document.getElementById(${failed_title_input}.toString())?.focus())",
                                        ()
                                    );
                                }
                            }
                            if failed_field == "description" {
                                if failed_body_revision.get() == sent_description_revision {
                                    failed_body_editing.set(true);
                                    raw!(
                                        "requestAnimationFrame(() => document.getElementById(${failed_body_input}.toString())?.focus())",
                                        ()
                                    );
                                }
                            }
                        };
                        let _save = async || {
                            let outcome = if field == "title" {
                                save_title(identifier, value, observed_seq).await
                            } else {
                                if field == "description" {
                                    save_description(identifier, value, observed_seq).await
                                } else {
                                    if field == "status" {
                                        save_status(identifier, value, observed_seq).await
                                    } else {
                                        save_priority(identifier, value, observed_seq).await
                                    }
                                }
                            };
                            busy.set(false);
                            let current = if outcome.1.is_some() {
                                outcome.1.clone().unwrap() >= seq.get()
                            } else {
                                true
                            };
                            if current {
                                if outcome.1.is_some() {
                                    seq.set(outcome.1.unwrap());
                                    title.set(outcome.2.unwrap());
                                    description.set(outcome.3.unwrap());
                                    status.set(outcome.4.unwrap());
                                    priority.set(outcome.5.unwrap());
                                }
                                if outcome.0.is_ok() {
                                    if field == "title" {
                                        if title_revision.get() == sent_title_revision {
                                            title_draft.set(title.get());
                                            title_editing.set(false);
                                        }
                                    } else {
                                        if title_draft.get() == before_title {
                                            title_draft.set(title.get());
                                        }
                                    }
                                    if field == "description" {
                                        if description_revision.get() == sent_description_revision {
                                            description_draft.set(description.get());
                                            description_editing.set(false);
                                        }
                                    } else {
                                        if description_draft.get() == before_description {
                                            description_draft.set(description.get());
                                        }
                                    }
                                } else {
                                    let reason = outcome.0.unwrap_err();
                                    if reason == "conflict" {
                                        message.set(
                                            "This issue changed. Your draft is still here."
                                                .to_owned(),
                                        );
                                        if field == "title" {
                                            if title_revision.get() == sent_title_revision {
                                                title_editing.set(true);
                                                raw!(
                                                    "requestAnimationFrame(() => document.getElementById(${title_input}.toString())?.focus())",
                                                    ()
                                                );
                                            }
                                        }
                                        if field == "description" {
                                            if description_revision.get()
                                                == sent_description_revision
                                            {
                                                description_editing.set(true);
                                                raw!(
                                                    "requestAnimationFrame(() => document.getElementById(${body_input}.toString())?.focus())",
                                                    ()
                                                );
                                            }
                                        }
                                    } else {
                                        if reason == "reauth" {
                                            raw!("window.location.assign(${login}.toString())", ());
                                        } else {
                                            if reason == "forbidden" {
                                                message.set(
                                                    "You no longer have permission to edit this issue."
                                                        .to_owned(),
                                                );
                                            } else {
                                                message.set(reason);
                                            }
                                            if field == "title" {
                                                if title_revision.get() == sent_title_revision {
                                                    title_editing.set(true);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        };
                        // Promise adaptation only; both completion paths are authored in Rust.
                        raw!(
                            "Promise.resolve().then(() => ${_save}()).catch(() => ${_failed}());",
                            ()
                        );
                    }
                }
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        format!("data-topcoat-on:{event_name}"),
        handler.into_evaluated_and_js().1,
    );
    attributes
}

pub(crate) fn editor<'a>(cx: &'a Cx, snapshot: &Snapshot, can_edit: bool) -> BoxView<'a> {
    let snapshot = snapshot.clone();
    view! { cx => editor_component(snapshot: snapshot, can_edit: can_edit) }.boxed()
}

#[component]
async fn editor_component(
    cx: &Cx,
    snapshot: Snapshot,
    can_edit: bool,
) -> topcoat::Result<impl View> {
    Ok(render_editor(cx, &snapshot, can_edit))
}

fn render_editor<'a>(cx: &'a Cx, snapshot: &Snapshot, can_edit: bool) -> BoxView<'a> {
    let identifier = snapshot.identifier.clone();
    let display_identifier = snapshot.identifier.clone();
    let display_title = snapshot.title.clone();
    let controls = Controls::new(cx, snapshot);
    let title = controls.title.clone();
    let description = controls.description.clone();
    let status = controls.status.clone();
    let priority = controls.priority.clone();
    let seq = controls.seq.clone();
    let title_draft = controls.title_draft.clone();
    let description_draft = controls.description_draft.clone();
    let title_revision = controls.title_revision.clone();
    let description_revision = controls.description_revision.clone();
    let title_editing = controls.title_editing.clone();
    let description_editing = controls.description_editing.clone();
    let busy = controls.busy.clone();
    let message = controls.message.clone();
    let title_input = format!("native-issue-title-input-{identifier}");
    let title_button = format!("native-issue-title-{identifier}");
    let body_input = format!("native-issue-body-input-{identifier}");
    let body_button = format!("native-issue-body-edit-{identifier}");
    let body_focus = body_input.clone();
    let title_blur = save_attributes(cx, &controls, Field::Title, "blur", None);
    let title_key = save_attributes(cx, &controls, Field::Title, "keydown", None);
    let body_key = save_attributes(cx, &controls, Field::Description, "keydown", None);
    let body_save = save_attributes(cx, &controls, Field::Description, "click", None);
    let relations = [
        (&snapshot.blocks, "Blocks"),
        (&snapshot.blocked_by, "Blocked by"),
        (&snapshot.relates_to, "Related"),
        (&snapshot.duplicates, "Duplicates"),
        (&snapshot.duplicated_by, "Duplicate of"),
    ]
    .into_iter()
    .filter(|(values, _)| !values.is_empty())
    .map(|(values, label)| {
        let links = values
            .iter()
            .map(|identifier| {
                let project = identifier
                    .split_once('-')
                    .map_or("", |(project, _)| project);
                let href = super::super::transport::mounted_url(
                    cx,
                    &format!("/{project}/issues/{identifier}"),
                );
                (identifier.clone(), href)
            })
            .collect::<Vec<_>>();
        (label, links)
    })
    .collect::<Vec<_>>();
    view! { cx =>
        <section class="native-issue-editor" data-native-issue-editor=(identifier)>
            <div class="native-issue-editor__heading"><span class="native-issue-editor__identifier">(display_identifier)</span>
                <output data-native-issue-seq="">$(seq.get())</output>
                <span role="status" :hidden=$(!busy.get())>"Saving…"</span>
            </div>
            <div class="native-issue-editor__content">
                if can_edit {
                    <button id=(title_button.clone()) class="native-issue-editor__title" :hidden=$(title_editing.get())
                        @click=$(|_event: Event| {
                            title_draft.set(title.get()); title_revision.increment(); title_editing.set(true);
                            raw!("requestAnimationFrame(() => document.getElementById(${title_input}.toString())?.focus())", ());
                        })>$(title.get())</button>
                    <input id=(title_input.clone()) type="text" aria-label="Issue title" class="native-issue-editor__title native-issue-editor__title-input"
                        :hidden=$(!title_editing.get()) :value=$(title_draft.get())
                        @input=$(|event: Event| {title_draft.set(event.target.value); title_revision.increment();})
                        (title_blur) (title_key)>
                    <div class="native-issue-editor__body-toolbar">
                        <button id=(body_button.clone()) :hidden=$(description_editing.get()) @click=$(|_event: Event| {
                            description_draft.set(description.get()); description_revision.increment(); description_editing.set(true);
                            raw!("requestAnimationFrame(() => document.getElementById(${body_focus}.toString())?.focus())", ());
                        })>"Edit"</button>
                        <button data-native-issue-body-save="" :hidden=$(!description_editing.get()) :disabled=$(busy.get()) (body_save)>"Save"</button>
                        <button data-native-issue-body-cancel="" :hidden=$(!description_editing.get()) @click=$(|_event: Event| {
                            description_editing.set(false); description_draft.set(description.get()); description_revision.increment();
                            raw!("requestAnimationFrame(() => document.getElementById(${body_button}.toString())?.focus())", ());
                        })>"Cancel"</button>
                    </div>
                    <textarea id=(body_input.clone()) aria-label="Issue description" placeholder="Add a description... (markdown supported)"
                        :hidden=$(!description_editing.get()) :value=$(description_draft.get())
                        @input=$(|event: Event| {description_draft.set(event.target.value); description_revision.increment();}) (body_key)></textarea>
                } else {
                    <h1 class="native-issue-editor__title">(display_title)</h1>
                }
                <pre class="native-issue-editor__preview" :hidden=$(description_editing.get())>$(description.get())</pre>
                <p class="native-issue-editor__empty" :hidden=$(!description.get().is_empty())>"No description"</p>
                <p data-native-issue-save-error="" role="alert" :hidden=$(message.get().is_empty())>$(message.get())</p>
            </div>
            <aside class="native-issue-editor__fields" aria-label="Issue fields">
                <section><h2>"Status"</h2><span data-native-issue-status="">$(status.get())</span>
                    if can_edit {
                        for (value, label) in [("backlog", "Backlog"), ("todo", "Todo"), ("active", "Active"), ("done", "Done"), ("cancelled", "Cancelled")] {
                            <button data-native-issue-status-option=(value) :disabled=$(busy.get())
                                (save_attributes(cx, &controls, Field::Status, "click", Some(value.into())))>(label)</button>
                        }
                    }
                </section>
                <section><h2>"Priority"</h2><span data-native-issue-priority="">$(priority.get())</span>
                    if can_edit {
                        for (value, label) in [("urgent", "Urgent"), ("high", "High"), ("medium", "Medium"), ("low", "Low"), ("none", "None")] {
                            <button data-native-issue-priority-option=(value) :disabled=$(busy.get())
                                (save_attributes(cx, &controls, Field::Priority, "click", Some(value.into())))>(label)</button>
                        }
                    }
                </section>
                for (label, links) in relations {
                    <section class="native-issue-editor__relations"><h2>(label)</h2>
                        for (identifier, href) in links {<a href=(href)>(identifier)</a>}
                    </section>
                }
            </aside>
        </section>
    }.boxed()
}
