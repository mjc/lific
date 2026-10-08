//! Rust-authored field controls and issue document composition.

use super::super::icons::UiIcon;
use topcoat::{
    context::Cx,
    runtime::{Event, Expr, Signal, Surrogated, expr, procedure, shard, signal},
    view::{Attributes, BoxView, Unescaped, View, ViewExt, component, view},
};

use super::{
    actions::{self, SaveOutcome, Snapshot},
    activity::native_issue_activity,
    model::Field,
};
use crate::db::models::{Priority, Status};

pub(crate) const STYLESHEET: &str = include_str!("controls.css");

type WireOutcome = (
    Result<String, String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
);
type ModuleAssignmentReplyValue =
    <super::module_assignment::ModuleAssignmentReply as Surrogated>::Surrogate;

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
            snapshot.blocks,
            snapshot.blocked_by,
            snapshot.relates_to,
            snapshot.duplicates,
            snapshot.duplicated_by,
        ),
        None => (
            result,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
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

#[derive(Clone)]
struct Controls {
    identifier: String,
    account_id: i64,
    issue_id: i64,
    seq: Signal<i64>,
    title: Signal<String>,
    description: Signal<String>,
    status: Signal<String>,
    priority: Signal<String>,
    blocks: Signal<Vec<String>>,
    blocked_by: Signal<Vec<String>>,
    relates_to: Signal<Vec<String>>,
    duplicates: Signal<Vec<String>>,
    duplicated_by: Signal<Vec<String>>,
    title_draft: Signal<String>,
    description_draft: Signal<String>,
    title_revision: Signal<i64>,
    description_revision: Signal<i64>,
    title_editing: Signal<bool>,
    description_editing: Signal<bool>,
    description_autofocused: Signal<bool>,
    busy: Signal<bool>,
    message: Signal<String>,
    properties_open: Signal<bool>,
    status_open: Signal<bool>,
    header_status_open: Signal<bool>,
    priority_open: Signal<bool>,
}

impl Controls {
    fn new(cx: &Cx, snapshot: &Snapshot, account_id: i64, issue_id: i64) -> Self {
        Self {
            identifier: snapshot.identifier.clone(),
            account_id,
            issue_id,
            seq: signal(cx, || snapshot.seq),
            title: signal(cx, || snapshot.title.clone()),
            description: signal(cx, || snapshot.description.clone()),
            status: signal(cx, || snapshot.status.as_str().to_owned()),
            priority: signal(cx, || snapshot.priority.as_str().to_owned()),
            blocks: signal(cx, || snapshot.blocks.clone()),
            blocked_by: signal(cx, || snapshot.blocked_by.clone()),
            relates_to: signal(cx, || snapshot.relates_to.clone()),
            duplicates: signal(cx, || snapshot.duplicates.clone()),
            duplicated_by: signal(cx, || snapshot.duplicated_by.clone()),
            title_draft: signal(cx, || snapshot.title.clone()),
            description_draft: signal(cx, || snapshot.description.clone()),
            title_revision: signal(cx, || 0i64),
            description_revision: signal(cx, || 0i64),
            title_editing: signal(cx, || false),
            description_editing: signal(cx, || false),
            description_autofocused: signal(cx, || false),
            busy: signal(cx, || false),
            message: signal(cx, String::new),
            properties_open: signal(cx, || false),
            status_open: signal(cx, || false),
            header_status_open: signal(cx, || false),
            priority_open: signal(cx, || false),
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
    let blocks = controls.blocks.clone();
    let blocked_by = controls.blocked_by.clone();
    let relates_to = controls.relates_to.clone();
    let duplicates = controls.duplicates.clone();
    let duplicated_by = controls.duplicated_by.clone();
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
    let status_open = controls.status_open.clone();
    let header_status_open = controls.header_status_open.clone();
    let priority_open = controls.priority_open.clone();
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
                    status_open.set(false);
                    header_status_open.set(false);
                    priority_open.set(false);
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
                        if field == "description" {
                            value != before_description
                        } else {
                            true
                        }
                    };
                    if !changed {
                        if field == "description" {
                            description_editing.set(false);
                        }
                    }
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
                                    blocks.set(outcome.6);
                                    blocked_by.set(outcome.7);
                                    relates_to.set(outcome.8);
                                    duplicates.set(outcome.9);
                                    duplicated_by.set(outcome.10);
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
    view! {
        cx =>
        editor_component(snapshot: snapshot, can_edit: can_edit, markdown: false)
    }
    .boxed()
}

#[component]
async fn editor_component(
    cx: &Cx,
    snapshot: Snapshot,
    can_edit: bool,
    markdown: bool,
) -> topcoat::Result<impl View> {
    let controls = Controls::new(cx, &snapshot, 0, 0);
    Ok(render_editor(
        cx, &snapshot, can_edit, markdown, &controls, false,
    ))
}

/// A disposable issue scope inside the persistent workspace shell.
pub(crate) fn document_region<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    can_edit: bool,
    project: &str,
    delete_request: &super::delete_menu::Request,
) -> BoxView<'a> {
    let snapshot = snapshot.clone();
    let project = project.to_owned();
    let delete_request = delete_request.clone();
    // Preserve drafts for this issue while giving another issue fresh field state.
    let issue_cx = cx.keyed(&snapshot.identifier);
    view! {
        issue_cx =>
        document_region_component(
            snapshot: snapshot,
            can_edit: can_edit,
            project: project,
            delete_request: delete_request
        )
    }
    .boxed()
}

fn document_views<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    can_edit: bool,
    project: &str,
    delete_request: &super::delete_menu::Request,
) -> (BoxView<'a>, BoxView<'a>) {
    let controls = Controls::new(
        cx,
        snapshot,
        delete_request.account_id,
        delete_request.issue_id,
    );
    (
        document_topbar(cx, &controls, project, can_edit, delete_request),
        render_editor(cx, snapshot, can_edit, true, &controls, true),
    )
}

#[component]
async fn document_region_component(
    cx: &Cx,
    snapshot: Snapshot,
    can_edit: bool,
    project: String,
    delete_request: super::delete_menu::Request,
) -> topcoat::Result<impl View> {
    let (topbar, content) = document_views(cx, &snapshot, can_edit, &project, &delete_request);
    Ok(super::super::home_shell::page_region(
        cx,
        content,
        Some(topbar),
        String::new(),
    ))
}

fn body_edit_attributes(cx: &Cx, controls: &Controls) -> Attributes {
    let description = controls.description.clone();
    let draft = controls.description_draft.clone();
    let revision = controls.description_revision.clone();
    let editing = controls.description_editing.clone();
    let busy = controls.busy.clone();
    let focus_revision = controls.description_revision.clone();
    let focus_editing = controls.description_editing.clone();
    let input = format!("native-issue-body-input-{}", controls.identifier);
    let handler = expr!(|_event: Event| {
        if !busy.get() {
            if !editing.get() {
                draft.set(description.get());
                revision.increment();
                editing.set(true);
                let expected_revision = revision.get();
                let _focus = || {
                    if focus_revision.get() == expected_revision {
                        if focus_editing.get() {
                            raw!("document.getElementById(${input}.toString())?.focus()", ());
                        }
                    }
                };
                raw!("requestAnimationFrame(${_focus})", ());
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

/// Browser event and focus primitives enter the same Rust-owned draft scope.
/// The mount runs once; later saves or cancellation cannot re-arm autofocus.
fn document_mount(cx: &Cx, controls: &Controls) -> Attributes {
    let description = controls.description.clone();
    let draft = controls.description_draft.clone();
    let revision = controls.description_revision.clone();
    let editing = controls.description_editing.clone();
    let autofocused = controls.description_autofocused.clone();
    let focus_revision = controls.description_revision.clone();
    let focus_editing = controls.description_editing.clone();
    let status_open = controls.status_open.clone();
    let header_status_open = controls.header_status_open.clone();
    let priority_open = controls.priority_open.clone();
    let disposed_revision = controls.description_revision.clone();
    let module_seq = controls.seq.clone();
    let module_account = controls.account_id;
    let module_issue = controls.issue_id;
    let module_title = controls.title.clone();
    let module_description = controls.description.clone();
    let module_status = controls.status.clone();
    let module_priority = controls.priority.clone();
    let module_blocks = controls.blocks.clone();
    let module_blocked_by = controls.blocked_by.clone();
    let module_relates_to = controls.relates_to.clone();
    let module_duplicates = controls.duplicates.clone();
    let module_duplicated_by = controls.duplicated_by.clone();
    let module_title_draft = controls.title_draft.clone();
    let module_description_draft = controls.description_draft.clone();
    let input = format!("native-issue-body-input-{}", controls.identifier);
    let handler = expr!(|_mount: Event| {
        let _dispose = || {
            disposed_revision.increment();
        };
        raw!(
            "cx.abortSignal.addEventListener('abort', ${_dispose}, {once:true});",
            ()
        );
        if !autofocused.get() {
            autofocused.set(true);
            if description.get().trim().is_empty() {
                draft.set(description.get());
                revision.increment();
                editing.set(true);
                let expected_revision = revision.get();
                let _focus = || {
                    if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        if focus_revision.get() == expected_revision {
                            if focus_editing.get() {
                                raw!("document.getElementById(${input}.toString())?.focus()", ());
                            }
                        }
                    }
                };
                raw!("requestAnimationFrame(${_focus})", ());
            }
        }
        let _dismiss = |inside_picker: topcoat::runtime::BoolSurrogate| {
            if !inside_picker {
                status_open.set(false);
                header_status_open.set(false);
                priority_open.set(false);
            }
        };
        raw!(
            "window.addEventListener('click', event => ${_dismiss}(cx.hydrate(Boolean(event.target?.closest?.('.native-issue-detail__picker')))), {signal:cx.abortSignal})",
            ()
        );
        let _module_applied = |reply: ModuleAssignmentReplyValue| {
            if reply.account_id == module_account {
                if reply.issue_id == module_issue {
                    if reply.seq >= module_seq.get() {
                        let was_clean_title = module_title_draft.get() == module_title.get();
                        let was_clean_description =
                            module_description_draft.get() == module_description.get();
                        module_seq.set(reply.seq);
                        if reply.canonical.is_some() {
                            let canonical = reply.canonical.unwrap();
                            module_title.set(canonical.title.clone());
                            module_description.set(canonical.description.clone());
                            module_status.set(canonical.status);
                            module_priority.set(canonical.priority);
                            module_blocks.set(canonical.blocks);
                            module_blocked_by.set(canonical.blocked_by);
                            module_relates_to.set(canonical.relates_to);
                            module_duplicates.set(canonical.duplicates);
                            module_duplicated_by.set(canonical.duplicated_by);
                            if was_clean_title {
                                module_title_draft.set(canonical.title);
                            }
                            if was_clean_description {
                                module_description_draft.set(canonical.description);
                            }
                        }
                    }
                }
            }
        };
        raw!(
            "window.addEventListener('lific:native-issue-module-applied', event => ${_module_applied}(event.detail), {signal:cx.abortSignal})",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

fn status_decoration(cx: &Cx, selected: Signal<String>, size: u32) -> BoxView<'_> {
    let variants = [
        Status::Backlog,
        Status::Todo,
        Status::Active,
        Status::Done,
        Status::Cancelled,
    ]
    .into_iter()
    .map(|status| {
        (
            status.as_str(),
            super::super::icons::status_icon(cx, status, size),
        )
    })
    .collect::<Vec<_>>();
    view! {
        cx =>
        for (value, icon) in variants {
            <span
                class="native-issue-detail__decoration"
                :hidden=$(selected.get() != value)
            >
                (icon)
            </span>
        }
    }
    .boxed()
}

fn priority_decoration(cx: &Cx, selected: Signal<String>, size: u32) -> BoxView<'_> {
    let variants = [
        Priority::Urgent,
        Priority::High,
        Priority::Medium,
        Priority::Low,
        Priority::None,
    ]
    .into_iter()
    .map(|priority| {
        (
            priority.as_str(),
            super::super::icons::priority_icon(cx, priority, size),
        )
    })
    .collect::<Vec<_>>();
    view! {
        cx =>
        for (value, icon) in variants {
            <span
                class="native-issue-detail__decoration"
                :hidden=$(selected.get() != value)
            >
                (icon)
            </span>
        }
    }
    .boxed()
}

fn priority_label(priority: Signal<String>) -> Expr<String> {
    expr!({
        let value = priority.get();
        if value == "none" {
            "No priority".to_owned()
        } else if value == "urgent" {
            "Urgent".to_owned()
        } else if value == "high" {
            "High".to_owned()
        } else if value == "medium" {
            "Medium".to_owned()
        } else if value == "low" {
            "Low".to_owned()
        } else {
            value.clone()
        }
    })
}

fn document_topbar<'a>(
    cx: &'a Cx,
    controls: &Controls,
    project: &str,
    can_edit: bool,
    delete_request: &super::delete_menu::Request,
) -> BoxView<'a> {
    let project_label = project.to_owned();
    let overview = format!("/{project}/overview");
    let identifier = controls.identifier.clone();
    let copy_project = project.to_owned();
    let copy_identifier = identifier.clone();
    let status = controls.status.clone();
    let header_status_open = controls.header_status_open.clone();
    let status_open = controls.status_open.clone();
    let priority_open = controls.priority_open.clone();
    let description = controls.description.clone();
    let editing = controls.description_editing.clone();
    let busy = controls.busy.clone();
    let properties_open = controls.properties_open.clone();
    let save = save_attributes(cx, controls, Field::Description, "click", None);
    let edit = body_edit_attributes(cx, controls);
    let status_options = status_options(cx, controls);
    let edit_id = format!("native-issue-body-edit-{}", controls.identifier);
    let (export_error, export_button) = super::export::toolbar_fragments(cx, &controls.identifier);
    let breadcrumb = super::list_return::breadcrumb(cx, project, &controls.identifier);
    let delete_menu = super::delete_menu::toolbar(cx, delete_request.clone(), can_edit, project);
    let keyboard = super::list_return::keyboard_mount(
        cx,
        project,
        &controls.identifier,
        controls.description_editing.clone(),
        controls.properties_open.clone(),
    );
    view! {
        cx =>
        <div class="native-issue-detail__topbar" (keyboard)>
            <div class="native-issue-detail__scope">
                <nav class="native-issue-detail__breadcrumbs" aria-label="Breadcrumb">
                    <ol>
                        <li data-hide-phone="">
                            <a
                                data-mono=""
                                (super::super::navigation::attrs(cx, &overview))
                                title=(project_label.clone())
                            >
                                <span data-label="">(project_label)</span>
                            </a>
                            <button
                                class="native-issue-detail__copy"
                                type="button"
                                aria-label=(format!("Copy {copy_project}"))
                                @click=$(|_event: Event| {
                                    raw!(
                                        "navigator.clipboard.writeText(${copy_project}.toString()).catch(() => {})",
                                        (),
                                    );
                                })
                            >
                                (super::super::icons::ui_icon(cx, UiIcon::Copy, 12))
                            </button>
                        </li>
                        <li data-separator="" data-hide-phone="" aria-hidden="true">
                            (super::super::icons::ui_icon(
                                cx,
                                UiIcon::BreadcrumbSeparator,
                                12,
                            ))
                        </li>
                        <li data-hide-phone="">(breadcrumb)</li>
                        <li data-separator="" data-hide-phone="" aria-hidden="true">
                            (super::super::icons::ui_icon(
                                cx,
                                UiIcon::BreadcrumbSeparator,
                                12,
                            ))
                        </li>
                        <li>
                            <span
                                data-mono=""
                                aria-current="page"
                                title=(identifier.clone())
                            >
                                <span data-label="">(identifier)</span>
                            </span>
                            <button
                                class="native-issue-detail__copy"
                                type="button"
                                aria-label=(format!("Copy {copy_identifier}"))
                                @click=$(|_event: Event| {
                                    raw!(
                                        "navigator.clipboard.writeText(${copy_identifier}.toString()).catch(() => {})",
                                        (),
                                    );
                                })
                            >
                                (super::super::icons::ui_icon(cx, UiIcon::Copy, 12))
                            </button>
                        </li>
                    </ol>
                </nav>
                <span aria-hidden="true">"/"</span>
                <div class="native-issue-detail__picker">
                    if can_edit {
                        <button
                            class="native-issue-detail__field-value"
                            type="button"
                            title="Change status"
                            aria-haspopup="menu"
                            :aria-expanded=$(if header_status_open.get() {
                                "true"
                            } else {
                                "false"
                            })
                            @click=$(|_event: Event| {
                                header_status_open.set(!header_status_open.get());
                                status_open.set(false);
                                priority_open.set(false);
                            })
                        >
                            (status_decoration(cx, status.clone(), 13))
                            <span :data-status=$(status.get())>$(status.get())</span>
                            (super::super::icons::ui_icon(cx, UiIcon::Expand, 11))
                        </button>
                        <div
                            class="native-issue-detail__menu"
                            role="menu"
                            :hidden=$(!header_status_open.get())
                        >
                            (status_options)
                        </div>
                    } else {
                        (status_decoration(cx, status.clone(), 13))
                        <span :data-status=$(status.get())>$(status.get())</span>
                        <span class="native-issue-detail__readonly">"Read-only"</span>
                    }
                </div>
            </div>
            <div class="native-issue-detail__actions">
                (export_error)
                if can_edit {
                    <div
                        class="native-issue-detail__mode"
                        role="radiogroup"
                        aria-label="Content view mode"
                        :hidden=$(description.get().trim().is_empty())
                    >
                        <button
                            id=(edit_id)
                            class="native-issue-detail__mode-option"
                            type="button"
                            role="radio"
                            aria-label="Edit"
                            title="Edit (E)"
                            :aria-checked=$(if editing.get() { "true" } else { "false" })
                            :disabled=$(busy.get())
                            (edit)
                        >
                            (super::super::icons::ui_icon(cx, UiIcon::Edit, 14))
                            <span>"Edit"</span>
                        </button>
                        <button
                            class="native-issue-detail__mode-option"
                            type="button"
                            role="radio"
                            aria-label="Preview"
                            title="Preview"
                            :aria-checked=$(if editing.get() { "false" } else { "true" })
                            :disabled=$(busy.get())
                            (save)
                        >
                            (super::super::icons::ui_icon(cx, UiIcon::Preview, 14))
                            <span>"Preview"</span>
                        </button>
                    </div>
                }
                <span class="native-issue-detail__save-status">
                    <span :hidden=$(!busy.get())>"Saving..."</span>
                </span>
                (export_button)
                (delete_menu)
                <button
                    id="native-issue-details-open"
                    class="native-issue-detail__properties-toggle"
                    type="button"
                    aria-label="Show details"
                    :aria-expanded=$(if properties_open.get() {
                        "true"
                    } else {
                        "false"
                    })
                    @click=$(|_event: Event| properties_open.set(true))
                >
                    (super::super::icons::ui_icon(cx, UiIcon::DetailsPanel, 16))
                </button>
            </div>
        </div>
    }.boxed()
}

fn status_options<'a>(cx: &'a Cx, controls: &Controls) -> BoxView<'a> {
    let controls = controls.clone();
    let status = controls.status.clone();
    let busy = controls.busy.clone();
    view! {
        cx =>
        for (value, label) in [
            ("backlog", "Backlog"),
            ("todo", "Todo"),
            ("active", "Active"),
            ("done", "Done"),
            ("cancelled", "Cancelled"),
        ] {
            <button
                type="button"
                role="menuitemradio"
                data-native-issue-status-option=(value)
                :aria-checked=$(if status.get() == value { "true" } else { "false" })
                :aria-selected=$(if status.get() == value { "true" } else { "false" })
                :disabled=$(busy.get())
                (save_attributes(
                    cx,
                    &controls,
                    Field::Status,
                    "click",
                    Some(value.into()),
                ))
            >
                (label)
            </button>
        }
    }
    .boxed()
}

#[shard("/__native_issue_edit/metadata")]
async fn native_issue_metadata(
    cx: &Cx,
    identifier: String,
    revision: i64,
    dates: bool,
    menu_signals: (Signal<bool>, Signal<bool>, Signal<bool>),
) -> topcoat::Result<impl View> {
    // The saved cursor invalidates this read; it never supplies authority.
    let _ = revision;
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    let user = super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let db = super::super::context::db(cx);
    let issue = super::super::session::read(
        cx,
        crate::services::issues::resolve_issue(db, &caller.identity, &identifier),
    )?;
    let can_edit = match crate::authz::require_role(
        db,
        &caller.identity,
        issue.project_id,
        crate::db::models::Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(crate::error::LificError::Forbidden(_)) => false,
        Err(error) => return super::super::session::read(cx, Err(error)),
    };
    let metadata = super::super::session::read(cx, super::route::metadata(cx, &issue))?;
    let module_request = super::module_assignment::ModuleRequest {
        account_id: user.id,
        issue_id: issue.id,
        identifier: issue.identifier.clone(),
        previous_module_id: issue.module_id,
        next_module_id: None,
    };
    Ok(metadata_view(
        cx,
        metadata,
        dates,
        can_edit,
        module_request,
        menu_signals,
    ))
}

fn metadata_view<'a>(
    cx: &'a Cx,
    metadata: super::route::DocumentMetadata,
    dates: bool,
    can_edit: bool,
    module_request: super::module_assignment::ModuleRequest,
    menu_signals: (Signal<bool>, Signal<bool>, Signal<bool>),
) -> BoxView<'a> {
    let waits = metadata
        .waits
        .iter()
        .cloned()
        .map(|wait| {
            let label = match wait.kind {
                crate::db::models::WaitKind::User => wait
                    .display_name
                    .filter(|name| !name.is_empty())
                    .or(wait.username)
                    .unwrap_or_else(|| "Deleted user".to_owned()),
                crate::db::models::WaitKind::Date => match (wait.earliest, wait.latest) {
                    (Some(from), Some(until)) if from != until => format!("{from} – {until}"),
                    (Some(from), _) => from,
                    _ => "Date".to_owned(),
                },
            };
            (label, wait.note)
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div class="native-issue-detail__metadata-read" style="display: contents;">
            if dates {
                <div class="native-issue-detail__divider" aria-hidden="true"></div>
                <section>
                    <h2>"Created"</h2>
                    <p class="native-issue-detail__date">
                        (date_text(cx, metadata.created_at))
                    </p>
                </section>
                <section>
                    <h2>"Updated"</h2>
                    <p class="native-issue-detail__date">
                        (date_text(cx, metadata.updated_at))
                    </p>
                </section>
            } else {
                <section>
                    <h2>"Module"</h2>
                    (super::module_assignment::field(
                        cx,
                        &metadata,
                        module_request,
                        can_edit,
                        menu_signals,
                    ))
                </section>
                <section>
                    <h2>"Labels"</h2>
                    if metadata.labels.is_empty() {
                        <span class="native-issue-detail__empty-value">"None"</span>
                    } else {
                        <div class="flex flex-wrap gap-1.5">
                            for (label, color) in metadata.labels {
                                (super::super::label_chip::render(
                                    cx,
                                    label,
                                    color.as_deref(),
                                ))
                            }
                        </div>
                    }
                </section>
                <div class="native-issue-detail__divider" aria-hidden="true"></div>
                if can_edit || !waits.is_empty() {
                    <section>
                        <h2>"Waiting on"</h2>
                        for (label, note) in waits {
                            <span>(label)</span>
                            if !note.is_empty() {
                                <small>(note)</small>
                            }
                        }
                    </section>
                }
            }
        </div>
    }
    .boxed()
}

fn date_text<'a>(cx: &'a Cx, timestamp: String) -> BoxView<'a> {
    let initial = timestamp.clone();
    let date = signal(cx, || initial);
    let datetime = timestamp.clone();
    view! {
        cx =>
        <time
            datetime=(datetime)
            @mount=$(|_event: Event| {
                // Browser-local locale/time-zone conversion is an Intl primitive;
                // the displayed value remains a Rust-owned framework signal.
                let local = raw!(
                    "cx.hydrate(new Date(${timestamp}.toString() + 'Z').toLocaleDateString('en-US', {month:'short',day:'numeric',year:'numeric',hour:'numeric',minute:'2-digit'}))",
                    String::new(),
                );
                date.set(local);
            })
        >
            $(date.get())
        </time>
    }.boxed()
}

#[shard("/__native_issue_edit/preview")]
async fn native_issue_markdown_preview(
    cx: &Cx,
    identifier: String,
    source: String,
) -> topcoat::Result<impl View> {
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    super::super::session::read(
        cx,
        crate::services::issues::resolve_issue(
            super::super::context::db(cx),
            &caller.identity,
            &identifier,
        ),
    )?;
    // Source is author-controlled input. Only the shared sanitizer's output is
    // promoted to markup; references do not resolve private resources here.
    let html =
        super::super::markdown::render(cx, &source, super::super::markdown::Scope::Private, &[]);
    Ok(view! { cx => (Unescaped::new_unchecked(html)) })
}

type RelationValues = (
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Vec<String>,
);
type RelationSignals = (
    Signal<Vec<String>>,
    Signal<Vec<String>>,
    Signal<Vec<String>>,
    Signal<Vec<String>>,
    Signal<Vec<String>>,
);

/// Render the captured outcome's graph, intersected with current read authority.
/// A later graph read must never replace the winning response's relation values.
#[shard("/__native_issue_edit/relations")]
async fn native_issue_relations(
    cx: &Cx,
    identifier: String,
    relations: RelationSignals,
) -> topcoat::Result<impl View> {
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let db = super::super::context::db(cx);
    let mut issue = super::super::session::read(
        cx,
        crate::services::issues::resolve_issue(db, &caller.identity, &identifier),
    )?;
    // Restored signals are untrusted. Reuse the shared target visibility policy
    // on these captured vectors, not the fresh issue's current graph.
    issue.blocks = relations.0.get();
    issue.blocked_by = relations.1.get();
    issue.relates_to = relations.2.get();
    issue.duplicates = relations.3.get();
    issue.duplicated_by = relations.4.get();
    // Validate restored destinations before rendering, including unrestricted
    // viewers for whom the target-visibility policy keeps every supplied value.
    for values in [
        &mut issue.blocks,
        &mut issue.blocked_by,
        &mut issue.relates_to,
        &mut issue.duplicates,
        &mut issue.duplicated_by,
    ] {
        values.retain(|identifier| {
            let Some((project, sequence)) = identifier.split_once('-') else {
                return false;
            };
            if crate::db::queries::validate_project_identifier(project).is_err() {
                return false;
            }
            sequence
                .parse::<i64>()
                .is_ok_and(|value| value > 0 && value.to_string() == sequence)
        });
    }
    super::super::session::read(
        cx,
        crate::services::issues::retain_visible_relations(
            db,
            &caller.identity,
            std::slice::from_mut(&mut issue),
        ),
    )?;
    Ok(relation_view(
        cx,
        (
            issue.blocks,
            issue.blocked_by,
            issue.relates_to,
            issue.duplicates,
            issue.duplicated_by,
        ),
    ))
}

fn relation_view<'a>(cx: &'a Cx, relations: RelationValues) -> BoxView<'a> {
    let groups = [
        (relations.1, "Blocked by"),
        (relations.0, "Blocks"),
        (relations.2, "Related"),
        (relations.3, "Duplicate of"),
        (relations.4, "Duplicated by"),
    ]
    .into_iter()
    .filter(|(values, _)| !values.is_empty())
    .map(|(values, label)| {
        let links = values
            .into_iter()
            .map(|identifier| {
                let project = identifier.split_once('-').unwrap().0;
                let href = format!("/{project}/issues/{identifier}");
                (identifier, href)
            })
            .collect::<Vec<_>>();
        (label, links)
    })
    .collect::<Vec<_>>();
    view! {
        cx =>
        for (label, links) in groups {
            <section class="native-issue-editor__relations">
                <h2>(label)</h2>
                for (identifier, href) in links {
                    <a (super::super::navigation::attrs(cx, &href))>(identifier)</a>
                }
            </section>
        }
    }
    .boxed()
}

fn editor_content<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    can_edit: bool,
    markdown: bool,
    document: bool,
    controls: &Controls,
) -> BoxView<'a> {
    let controls = controls.clone();
    let identifier = snapshot.identifier.clone();
    let preview_identifier = identifier.clone();
    let activity_identifier = identifier.clone();
    let display_title = snapshot.title.clone();
    let initial_description = snapshot.description.clone();
    let title = controls.title.clone();
    let description = controls.description.clone();
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
    let empty_edit = body_edit_attributes(cx, &controls);
    view! {
        cx =>
        <div class="native-issue-editor__content">
            if can_edit {
                <button
                    id=(title_button.clone())
                    class="native-issue-editor__title"
                    :hidden=$(title_editing.get())
                    @click=$(|_event: Event| {
                        title_draft.set(title.get());
                        title_revision.increment();
                        title_editing.set(true);
                        raw!(
                            "requestAnimationFrame(() => document.getElementById(${title_input}.toString())?.focus())",
                            (),
                        );
                    })
                >
                    $(title.get())
                </button>
                <input
                    id=(title_input.clone())
                    type="text"
                    aria-label="Issue title"
                    class="native-issue-editor__title native-issue-editor__title-input"
                    :hidden=$(!title_editing.get())
                    :value=$(title_draft.get())
                    @input=$(|event: Event| {
                        title_draft.set(event.target.value);
                        title_revision.increment();
                    })
                    (title_blur)
                    (title_key)
                >
                <div
                    class="native-issue-editor__body-toolbar"
                    :hidden=$(if document { !description_editing.get() } else { false })
                >
                    if !document {
                        <button
                            id=(body_button.clone())
                            :hidden=$(description_editing.get())
                            @click=$(|_event: Event| {
                                description_draft.set(description.get());
                                description_revision.increment();
                                description_editing.set(true);
                                raw!(
                                    "requestAnimationFrame(() => document.getElementById(${body_focus}.toString())?.focus())",
                                    (),
                                );
                            })
                        >
                            "Edit"
                        </button>
                    }
                    <button
                        data-native-issue-body-save=""
                        :hidden=$(!description_editing.get())
                        :disabled=$(busy.get())
                        (body_save)
                    >
                        "Save"
                    </button>
                    <button
                        data-native-issue-body-cancel=""
                        :hidden=$(!description_editing.get())
                        @click=$(|_event: Event| {
                            description_editing.set(false);
                            description_draft.set(description.get());
                            description_revision.increment();
                            raw!(
                                "requestAnimationFrame(() => document.getElementById(${body_button}.toString())?.focus())",
                                (),
                            );
                        })
                    >
                        "Cancel"
                    </button>
                </div>
                <textarea
                    id=(body_input.clone())
                    aria-label="Issue description"
                    placeholder="Add a description... (markdown supported)"
                    :hidden=$(!description_editing.get())
                    :value=$(description_draft.get())
                    @input=$(|event: Event| {
                        description_draft.set(event.target.value);
                        description_revision.increment();
                    })
                    (body_key)
                >
                    (initial_description)
                </textarea>
            } else {
                <h1 class="native-issue-editor__title">(display_title)</h1>
            }
            if markdown {
                <div
                    class="native-issue-editor__preview tc-markdown"
                    :hidden=$(description_editing.get())
                >
                    native_issue_markdown_preview(
                        identifier: preview_identifier,
                        source: $(description.get())
                    )
                </div>
            } else {
                <pre
                    class="native-issue-editor__preview"
                    :hidden=$(description_editing.get())
                >
                    $(description.get())
                </pre>
            }
            if document && can_edit {
                <button
                    type="button"
                    class="native-issue-editor__empty native-issue-editor__empty-edit"
                    :hidden=$(if description_editing.get() {
                        true
                    } else {
                        !description.get().trim().is_empty()
                    })
                    :disabled=$(busy.get())
                    (empty_edit)
                >
                    "Click to add a description..."
                </button>
            } else {
                <p
                    class="native-issue-editor__empty"
                    :hidden=$(!description.get().trim().is_empty())
                >
                    "No description"
                </p>
            }
            <p
                data-native-issue-save-error=""
                role="alert"
                :hidden=$(message.get().is_empty())
            >
                $(message.get())
            </p>
            if document {
                native_issue_activity(
                    identifier: activity_identifier,
                    revision: $(seq.get())
                )
            }
        </div>
    }.boxed()
}

fn editor_fields<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    can_edit: bool,
    document: bool,
    controls: &Controls,
) -> BoxView<'a> {
    let controls = controls.clone();
    let identifier = snapshot.identifier.clone();
    let relations_identifier = identifier.clone();
    let relation_signals = (
        controls.blocks.clone(),
        controls.blocked_by.clone(),
        controls.relates_to.clone(),
        controls.duplicates.clone(),
        controls.duplicated_by.clone(),
    );
    let metadata_identifier = identifier.clone();
    let dates_identifier = identifier;
    let status = controls.status.clone();
    let priority = controls.priority.clone();
    let seq = controls.seq.clone();
    let status_open = controls.status_open.clone();
    let header_status_open = controls.header_status_open.clone();
    let priority_open = controls.priority_open.clone();
    let module_menu_signals = (
        status_open.clone(),
        header_status_open.clone(),
        priority_open.clone(),
    );
    let properties_open = controls.properties_open.clone();
    let busy = controls.busy.clone();
    let editable_priority_label = priority_label(priority.clone());
    let readonly_priority_label = priority_label(priority.clone());
    let compact_priority_label = priority_label(priority.clone());
    let initial_relations = (
        snapshot.blocks.clone(),
        snapshot.blocked_by.clone(),
        snapshot.relates_to.clone(),
        snapshot.duplicates.clone(),
        snapshot.duplicated_by.clone(),
    );
    view! {
        cx =>
        <aside class="native-issue-editor__fields" aria-label="Issue fields">
            if document {
                <button
                    class="native-issue-detail__properties-close"
                    type="button"
                    aria-label="Close details"
                    @click=$(|_event: Event| {
                        properties_open.set(false);
                        raw!(
                            "requestAnimationFrame(() => document.getElementById('native-issue-details-open')?.focus())",
                            (),
                        );
                    })
                >
                    (super::super::icons::ui_icon(cx, UiIcon::Close, 18))
                </button>
            }
            <section>
                <h2>"Status"</h2>
                if document && can_edit {
                    <div class="native-issue-detail__picker">
                        <button
                            class="native-issue-detail__field-value"
                            type="button"
                            aria-label="Change issue status"
                            aria-haspopup="menu"
                            :aria-expanded=$(if status_open.get() {
                                "true"
                            } else {
                                "false"
                            })
                            @click=$(|_event: Event| {
                                status_open.set(!status_open.get());
                                priority_open.set(false);
                                header_status_open.set(false);
                            })
                        >
                            (status_decoration(cx, status.clone(), 14))
                            <span data-native-issue-status="">$(status.get())</span>
                        </button>
                        <div
                            class="native-issue-detail__menu"
                            role="menu"
                            :hidden=$(!status_open.get())
                        >
                            (status_options(cx, &controls))
                        </div>
                    </div>
                } else if document {
                    <span class="native-issue-detail__field-value">
                        (status_decoration(cx, status.clone(), 14))
                        <span data-native-issue-status="">$(status.get())</span>
                    </span>
                } else {
                    <span class="capitalize" data-native-issue-status="">
                        $(status.get())
                    </span>
                }
                if can_edit && !document {
                    for (value, label) in [
                        ("backlog", "Backlog"),
                        ("todo", "Todo"),
                        ("active", "Active"),
                        ("done", "Done"),
                        ("cancelled", "Cancelled"),
                    ] {
                        <button
                            data-native-issue-status-option=(value)
                            :disabled=$(busy.get())
                            (save_attributes(
                                cx,
                                &controls,
                                Field::Status,
                                "click",
                                Some(value.into()),
                            ))
                        >
                            (label)
                        </button>
                    }
                }
            </section>
            <section>
                <h2>"Priority"</h2>
                if document && can_edit {
                    <div class="native-issue-detail__picker">
                        <button
                            class="native-issue-detail__field-value"
                            type="button"
                            aria-label="Change issue priority"
                            aria-haspopup="menu"
                            :aria-expanded=$(if priority_open.get() {
                                "true"
                            } else {
                                "false"
                            })
                            @click=$(|_event: Event| {
                                priority_open.set(!priority_open.get());
                                status_open.set(false);
                                header_status_open.set(false);
                            })
                        >
                            (priority_decoration(cx, priority.clone(), 14))
                            <span
                                data-native-issue-priority=""
                                :data-priority=$(priority.get())
                            >
                                $(editable_priority_label)
                            </span>
                        </button>
                        <div
                            class="native-issue-detail__menu"
                            role="menu"
                            :hidden=$(!priority_open.get())
                        >
                            for (value, label) in [
                                ("urgent", "Urgent"),
                                ("high", "High"),
                                ("medium", "Medium"),
                                ("low", "Low"),
                                ("none", "No priority"),
                            ] {
                                <button
                                    type="button"
                                    role="menuitemradio"
                                    data-native-issue-priority-option=(value)
                                    :aria-checked=$(if priority.get() == value {
                                        "true"
                                    } else {
                                        "false"
                                    })
                                    :disabled=$(busy.get())
                                    (save_attributes(
                                        cx,
                                        &controls,
                                        Field::Priority,
                                        "click",
                                        Some(value.into()),
                                    ))
                                >
                                    (label)
                                </button>
                            }
                        </div>
                    </div>
                } else if document {
                    <span class="native-issue-detail__field-value">
                        (priority_decoration(cx, priority.clone(), 14))
                        <span
                            data-native-issue-priority=""
                            :data-priority=$(priority.get())
                        >
                            $(readonly_priority_label)
                        </span>
                    </span>
                } else {
                    <span class="capitalize" data-native-issue-priority="">
                        $(compact_priority_label)
                    </span>
                }
                if can_edit && !document {
                    for (value, label) in [
                        ("urgent", "Urgent"),
                        ("high", "High"),
                        ("medium", "Medium"),
                        ("low", "Low"),
                        ("none", "None"),
                    ] {
                        <button
                            data-native-issue-priority-option=(value)
                            :disabled=$(busy.get())
                            (save_attributes(
                                cx,
                                &controls,
                                Field::Priority,
                                "click",
                                Some(value.into()),
                            ))
                        >
                            (label)
                        </button>
                    }
                }
            </section>
            if document {
                native_issue_metadata(
                    identifier: metadata_identifier,
                    revision: $(seq.get()),
                    dates: false,
                    menu_signals: module_menu_signals.clone()
                )
            }
            if document {
                native_issue_relations(
                    identifier: relations_identifier,
                    relations: relation_signals
                )
            } else {
                (relation_view(cx, initial_relations))
            }
            if document {
                native_issue_metadata(
                    identifier: dates_identifier,
                    revision: $(seq.get()),
                    dates: true,
                    menu_signals: module_menu_signals
                )
            }
        </aside>
    }.boxed()
}

fn render_editor<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    can_edit: bool,
    markdown: bool,
    controls: &Controls,
    document: bool,
) -> BoxView<'a> {
    let identifier = snapshot.identifier.clone();
    let properties_open = controls.properties_open.clone();
    let seq = controls.seq.clone();
    let busy = controls.busy.clone();
    let mounted = if document && can_edit {
        document_mount(cx, controls)
    } else {
        Attributes::with_capacity(0)
    };
    let content = editor_content(cx, snapshot, can_edit, markdown, document, controls);
    let fields = editor_fields(cx, snapshot, can_edit, document, controls);
    let display_identifier = snapshot.identifier.clone();
    view! {
        cx =>
        <section
            class=(if document {
                "native-issue-editor native-issue-detail"
            } else {
                "native-issue-editor"
            })
            data-native-issue-editor=(identifier)
            :data-native-issue-properties-open=$(if properties_open.get() {
                "true"
            } else {
                "false"
            })
            (mounted)
        >
            <div class="native-issue-editor__heading">
                <span class="native-issue-editor__identifier">
                    (display_identifier)
                </span>
                <output data-native-issue-seq="">$(seq.get())</output>
                <span role="status" :hidden=$(!busy.get())>"Saving…"</span>
            </div>
            (content)
            (fields)
            if document {
                <button
                    class="native-issue-detail__properties-backdrop"
                    type="button"
                    aria-label="Close details"
                    :hidden=$(!properties_open.get())
                    @click=$(|_event: Event| {
                        properties_open.set(false);
                        raw!(
                            "requestAnimationFrame(() => document.getElementById('native-issue-details-open')?.focus())",
                            (),
                        );
                    })
                ></button>
            }
        </section>
    }
    .boxed()
}
