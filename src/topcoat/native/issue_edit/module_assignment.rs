//! Typed account-level requests for native IssueDetail module assignment.

use super::super::{context, session};
use crate::{
    db::models::{Issue, Module, UpdateIssue},
    error::LificError,
    realtime::RealtimeHub,
    services,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, record, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) fn field<'a>(
    cx: &'a Cx,
    metadata: &super::route::DocumentMetadata,
    request: ModuleRequest,
    can_edit: bool,
    menus: (Signal<bool>, Signal<bool>, Signal<bool>),
) -> BoxView<'a> {
    let module_id = request.previous_module_id;
    let module_label = metadata.module.clone();
    let current_emoji = metadata
        .modules
        .iter()
        .find(|module| Some(module.id) == module_id)
        .and_then(|module| module.emoji.as_deref());
    let current_icon = super::super::icons::project_icon(cx, current_emoji, 14);
    let open_module_path =
        module_id.map(|id| format!("/{}/modules/{id}", metadata.project_identifier));
    let open_module_href = open_module_path
        .as_ref()
        .map(|path| super::super::transport::mounted_url(cx, path));
    let open_module_attributes = open_module_path.as_ref().map(|path| {
        let mut attributes = super::super::navigation::attrs(cx, path);
        let _ = attributes.remove("href");
        attributes
    });
    if !can_edit {
        return view! {
            cx =>
            <span
                class=(if module_id.is_none() {
                    "native-issue-detail__empty-value"
                } else {
                    ""
                })
            >
                (current_icon)
                (module_label)
            </span>
            if let (Some(href), Some(attributes)) = (
                open_module_href,
                open_module_attributes,
            ) {
                <a
                    class="inline-flex items-center gap-1 ml-1 text-xs text-[var(--text-faint)] hover:text-[var(--accent)]"
                    href=(href)
                    title="Open module"
                    aria-label="Open module"
                    (attributes)
                >
                    (super::super::icons::ui_icon(
                        cx,
                        super::super::icons::UiIcon::OpenEntity,
                        13,
                    ))
                </a>
            }
        }
        .boxed();
    }

    let open = signal(cx, || false);
    let status_open = menus.0;
    let header_status_open = menus.1;
    let priority_open = menus.2;
    let toggle_open = open.clone();
    let browser = super::super::browser::bindings();
    let toggle = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            if !toggle_open.get() {
                status_open.set(false);
                header_status_open.set(false);
                priority_open.set(false);
            }
            toggle_open.set(!toggle_open.get());
        }
    });
    let mut toggle_attributes = Attributes::with_capacity(1);
    toggle_attributes.insert(
        cx,
        "data-topcoat-on:click",
        toggle.into_evaluated_and_js().1,
    );
    let current_label = module_label;
    let dismiss_open = open.clone();
    let dismiss = expr!(|_mount: Event| {
        let _outside = |inside_picker: topcoat::runtime::BoolSurrogate| {
            if !inside_picker {
                dismiss_open.set(false);
            }
        };
        raw!(
            "window.addEventListener('click', event => ${_outside}(cx.hydrate(Boolean(event.target?.closest?.('[data-native-issue-module-picker]')))), {signal:cx.abortSignal})",
            ()
        );
    });
    let mut mount_attributes = Attributes::with_capacity(1);
    mount_attributes.insert(
        cx,
        "data-topcoat-on:mount",
        dismiss.into_evaluated_and_js().1,
    );
    let options = metadata
        .modules
        .iter()
        .map(|module| option(cx, &open, &request, Some(module)))
        .collect::<Vec<_>>();
    let none_option = option(cx, &open, &request, None);
    view! {
        cx =>
        <div
            class="native-issue-detail__picker relative min-w-0"
            data-native-issue-module-picker=""
            (mount_attributes)
        >
            <div class="flex items-center gap-1">
                <button
                    class="inline-flex min-w-0 items-center gap-1.5 rounded px-1.5 py-1 text-left text-sm text-[var(--text)] hover:bg-[var(--bg-subtle)]"
                    aria-haspopup="listbox"
                    :aria-expanded=$(open.get())
                    (toggle_attributes)
                >
                    (current_icon)
                    <span
                        class=(if module_id.is_none() {
                            "native-issue-detail__empty-value"
                        } else {
                            ""
                        })
                    >
                        (current_label)
                    </span>
                </button>
                if let (Some(href), Some(attributes)) = (
                    open_module_href,
                    open_module_attributes,
                ) {
                    <a
                        class="inline-flex size-6 items-center justify-center rounded text-[var(--text-faint)] hover:bg-[var(--bg-subtle)] hover:text-[var(--accent)]"
                        href=(href)
                        title="Open module"
                        aria-label="Open module"
                        (attributes)
                    >
                        (super::super::icons::ui_icon(
                            cx,
                            super::super::icons::UiIcon::OpenEntity,
                            13,
                        ))
                    </a>
                }
            </div>
            <div
                class="absolute left-0 top-full z-30 mt-1 min-w-48 rounded-lg border border-[var(--border)] bg-[var(--bg)] p-1 shadow-lg"
                role="listbox"
                aria-label="Choose module"
                :hidden=$(!open.get())
                data-native-issue-module-options=""
            >
                (none_option)
                for option in options {
                    (option)
                }
            </div>
        </div>
    }
    .boxed()
}

fn option<'a>(
    cx: &'a Cx,
    open: &topcoat::runtime::Signal<bool>,
    request: &ModuleRequest,
    module: Option<&Module>,
) -> BoxView<'a> {
    let next_module_id = module.map(|module| module.id);
    let request = ModuleRequest {
        next_module_id,
        ..request.clone()
    };
    let label = module.map_or_else(|| "None".to_owned(), |module| module.name.clone());
    let emoji = module.and_then(|module| module.emoji.as_deref());
    let previous_module_id = request.previous_module_id;
    let changed = previous_module_id != next_module_id;
    let select_open = open.clone();
    let browser = super::super::browser::bindings();
    let select = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            if changed {
                let accepted = raw!(
                    "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-issue-module-request', {detail:${request}, cancelable:true})))",
                    false
                );
                if accepted {
                    select_open.set(false);
                }
            } else {
                select_open.set(false);
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        select.into_evaluated_and_js().1,
    );
    let selected = !changed;
    let value = next_module_id.map_or_else(|| "none".to_owned(), |id| id.to_string());
    let icon = super::super::icons::project_icon(cx, emoji, 14);
    view! {
        cx =>
        <button
            class=(if selected {
                "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-[var(--accent)] bg-[var(--accent-subtle)]"
            } else {
                "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-[var(--text)] hover:bg-[var(--bg-subtle)]"
            })
            role="option"
            :aria-selected=$(selected)
            data-native-issue-module-option=(value)
            (attributes)
        >
            (icon)
            <span>(label)</span>
        </button>
    }
    .boxed()
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct ModuleRequest {
    pub account_id: i64,
    pub issue_id: i64,
    pub identifier: String,
    pub previous_module_id: Option<i64>,
    pub next_module_id: Option<i64>,
}

#[record]
#[derive(Clone)]
pub(crate) struct ModuleAssignmentReply {
    pub status: Result<String, String>,
    pub account_id: i64,
    pub issue_id: i64,
    pub seq: i64,
    pub module_id: Option<i64>,
    pub module_label: String,
    pub canonical: Option<ModuleAssignmentSnapshot>,
}

#[record]
#[derive(Clone)]
pub(crate) struct ModuleAssignmentSnapshot {
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub relates_to: Vec<String>,
    pub duplicates: Vec<String>,
    pub duplicated_by: Vec<String>,
}

impl ModuleAssignmentSnapshot {
    fn from_issue(issue: &Issue) -> Self {
        Self {
            title: issue.title.clone(),
            description: issue.description.clone(),
            status: issue.status.as_str().to_owned(),
            priority: issue.priority.as_str().to_owned(),
            blocks: issue.blocks.clone(),
            blocked_by: issue.blocked_by.clone(),
            relates_to: issue.relates_to.clone(),
            duplicates: issue.duplicates.clone(),
            duplicated_by: issue.duplicated_by.clone(),
        }
    }
}

#[procedure("/__native_issue_edit/assign_module")]
pub(crate) async fn assign_module(
    cx: &Cx,
    request: ModuleRequest,
) -> topcoat::Result<ModuleAssignmentReply> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) => user,
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return session::read(cx, Err(LificError::Forbidden(message)));
        }
        Err(error) => return session::read(cx, Err(error)),
    };
    if user.id != request.account_id {
        return Ok(failed(&request, "insufficient project permissions"));
    }

    let db = context::db(cx);
    let issue = match services::issues::resolve_issue(db, &caller.identity, &request.identifier) {
        Ok(issue) => issue,
        Err(LificError::Forbidden(_)) => {
            return Ok(failed(&request, "insufficient project permissions"));
        }
        Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(error) => return session::read(cx, Err(error)),
    };
    if issue.id != request.issue_id {
        return Ok(failed(&request, "not found"));
    }
    if let Some(module_id) = request.next_module_id {
        let belongs_to_project = {
            let conn = db.read()?;
            crate::db::queries::list_modules(&conn, issue.project_id)?
                .iter()
                .any(|module| module.id == module_id)
        };
        if !belongs_to_project {
            return Ok(failed(&request, "module does not belong to this project"));
        }
    }

    let saved = match caller
        .scope(async {
            services::issues::commit_issue_update(
                db,
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                issue.id,
                UpdateIssue {
                    module_id: Some(request.next_module_id),
                    ..Default::default()
                },
            )
        })
        .await
    {
        Ok(saved) => saved,
        Err(LificError::Forbidden(_)) => {
            return Ok(failed(&request, "insufficient project permissions"));
        }
        Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
        Err(LificError::BadRequest(message)) => return Ok(failed(&request, &message)),
        Err(error) => return Err(error.into()),
    };
    let module_label = match saved.module_id {
        None => "None".to_owned(),
        Some(id) => {
            let conn = db.read()?;
            match crate::db::queries::get_module_name(&conn, id) {
                Ok(name) => name,
                Err(LificError::NotFound(_)) => "Unknown".to_owned(),
                Err(error) => return Err(error.into()),
            }
        }
    };
    Ok(ModuleAssignmentReply {
        status: Ok("saved".to_owned()),
        account_id: user.id,
        issue_id: saved.id,
        seq: saved.seq,
        module_id: saved.module_id,
        module_label,
        canonical: Some(ModuleAssignmentSnapshot::from_issue(&saved)),
    })
}

fn failed(request: &ModuleRequest, message: &str) -> ModuleAssignmentReply {
    ModuleAssignmentReply {
        status: Err(message.to_owned()),
        account_id: request.account_id,
        issue_id: request.issue_id,
        seq: 0,
        module_id: request.previous_module_id,
        module_label: String::new(),
        canonical: None,
    }
}
