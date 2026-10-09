use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::{browser, context, navigation, session, transport};
use crate::{
    db::models::{Plan, PlanStepNode, Project, Role, UpdatePlan},
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Expr, Js, Signal, expr, procedure, shard, signal},
    view::{Attributes, BoxView, Unescaped, View, ViewExt, component, view},
};

#[derive(Clone)]
struct PlanEditor {
    revision: Signal<i64>,
    canonical_title: Signal<String>,
    title_draft: Signal<String>,
    title_editing: Signal<bool>,
    step_title_target: Signal<i64>,
    step_title_draft: Signal<String>,
    step_description_target: Signal<i64>,
    step_description_draft: Signal<String>,
    busy: Signal<bool>,
    message: Signal<String>,
}

struct PlanAction<'a> {
    action: &'a str,
    value: &'a str,
    label: &'a str,
    class: &'a str,
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    project: &Project,
    account: i64,
    plan_id: &str,
) -> topcoat::Result<BoxView<'a>> {
    let id = plan_id
        .parse::<i64>()
        .map_err(|_| topcoat::router::error::not_found())?;
    let project_identifier = project.identifier.clone();
    Ok(view! {
        cx =>
        native_plan_detail_body(
            account: account,
            project: project_identifier,
            plan_id: id
        )
    }
    .boxed())
}

#[shard("/__native_plans/detail")]
async fn native_plan_detail_body(
    cx: &Cx,
    account: i64,
    project: String,
    plan_id: i64,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let plan = session::read(
        cx,
        crate::services::plans::get(context::db(cx), &caller.identity, plan_id),
    )?;
    let project_row = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            crate::db::queries::get_project(
                &conn,
                crate::db::queries::resolve_project_identifier(&conn, &project)?,
            )
        })(),
    )?;
    if plan.project_id != project_row.id {
        return session::read(
            cx,
            Err(LificError::NotFound(format!("plan {plan_id} not found"))),
        );
    }
    let owner = cx.keyed(format!("native-plan-owner-{account}-{plan_id}"));
    Ok(view! {
        owner =>
        plan_detail_owner(account: account, project: project, plan: plan)
    }
    .boxed())
}

#[component]
async fn plan_detail_owner(
    cx: &Cx,
    account: i64,
    project: String,
    plan: Plan,
) -> topcoat::Result<impl View> {
    let revision = signal(cx, || 0_i64);
    let canonical_title = signal(cx, || plan.title.clone());
    let busy = signal(cx, || false);
    let message = signal(cx, String::new);
    let title_draft = signal(cx, || plan.title.clone());
    let title_editing = signal(cx, || false);
    let step_title_target = signal(cx, || 0_i64);
    let step_title_draft = signal(cx, String::new);
    let step_description_target = signal(cx, || 0_i64);
    let step_description_draft = signal(cx, String::new);
    Ok(view! {
        cx =>
        <div data-native-plan-owner=(plan.identifier.clone())>
            <p
                class="text-body-sm text-[var(--error)] px-6 pt-3"
                role="alert"
                :hidden=$(message.get().is_empty())
            >
                $(message.get())
            </p>
            native_plan_saved(
                account: account,
                project: project,
                plan_id: plan.id,
                revision_value: $(revision.get()),
                revision_owner: revision,
                title_state: (canonical_title, title_draft, title_editing),
                step_title_target: step_title_target,
                step_title_draft: step_title_draft,
                step_description_target: step_description_target,
                step_description_draft: step_description_draft,
                busy: busy,
                message: message
            )
        </div>
    })
}

use shards::native_plan_saved;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat adds its request context to the flat shard transport arguments"
)]
mod shards {
    use super::*;

    #[shard("/__native_plans/saved")]
    pub(super) async fn native_plan_saved(
        cx: &Cx,
        account: i64,
        project: String,
        plan_id: i64,
        revision_value: i64,
        revision_owner: Signal<i64>,
        title_state: (Signal<String>, Signal<String>, Signal<bool>),
        step_title_target: Signal<i64>,
        step_title_draft: Signal<String>,
        step_description_target: Signal<i64>,
        step_description_draft: Signal<String>,
        busy: Signal<bool>,
        message: Signal<String>,
    ) -> topcoat::Result<impl View> {
        let _ = revision_value;
        let (canonical_title, title_draft, title_editing) = title_state;
        let caller = session::read(cx, context::caller(cx))?;
        let user = session::read(cx, crate::api::require_user(&caller.identity))?;
        if user.id != account {
            return session::read(
                cx,
                Err(LificError::Forbidden(
                    "Your account changed. Reload this page.".into(),
                )),
            );
        }
        let plan = session::read(
            cx,
            crate::services::plans::get(context::db(cx), &caller.identity, plan_id),
        )?;
        let project_row = session::read(
            cx,
            (|| {
                let conn = context::db(cx).read()?;
                crate::db::queries::get_project(
                    &conn,
                    crate::db::queries::resolve_project_identifier(&conn, &project)?,
                )
            })(),
        )?;
        if plan.project_id != project_row.id {
            return session::read(
                cx,
                Err(LificError::NotFound(format!("plan {plan_id} not found"))),
            );
        }
        let can_edit = match crate::authz::require_role(
            context::db(cx),
            &caller.identity,
            plan.project_id,
            Role::Maintainer,
        ) {
            Ok(()) => true,
            Err(LificError::Forbidden(_)) => false,
            Err(error) => return session::read(cx, Err(error)),
        };
        let activity = session::read(
            cx,
            crate::services::activity::list_activity(
                context::db(cx),
                &caller.identity,
                crate::db::queries::activity::ActivityScope::Plan(plan.id),
                None,
                Some(100),
                None,
            ),
        )?
        .items;
        Ok(render_detail(
            cx,
            account,
            &project,
            plan,
            can_edit,
            activity,
            PlanEditor {
                revision: revision_owner,
                canonical_title,
                title_draft,
                title_editing,
                step_title_target,
                step_title_draft,
                step_description_target,
                step_description_draft,
                busy,
                message,
            },
        ))
    }
}

fn render_detail<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    plan: Plan,
    can_edit: bool,
    activity: Vec<crate::db::models::Activity>,
    editor: PlanEditor,
) -> BoxView<'a> {
    prepare_step_expansion_state(cx, &plan.steps);
    let steps = plan
        .steps
        .iter()
        .cloned()
        .map(|step| {
            step_node(
                cx,
                account,
                project,
                plan.id,
                step,
                can_edit,
                0,
                editor.clone(),
            )
        })
        .collect::<Vec<_>>();
    let progress = if plan.step_count > 0 {
        plan.done_count as f64 / plan.step_count as f64
    } else {
        0.0
    };
    let progress_width = format!("width: {}%", progress * 100.0);
    let back = navigation::attrs(cx, &format!("/{project}/plans"));
    let identifier = plan.identifier.clone();
    let title = plan.title.clone();
    let status = plan.status.clone();
    let anchor = plan.anchor_identifier.clone();
    let anchor_href = anchor
        .as_deref()
        .map(|value| issue_href(cx, project, value));
    let progress_text = format!("{}/{}", plan.done_count, plan.step_count);
    let dates = (plan.created_at.clone(), plan.updated_at.clone());
    let title_editor = if can_edit {
        Some(title_form(
            cx,
            account,
            project,
            plan.id,
            editor.canonical_title.clone(),
            editor.clone(),
        ))
    } else {
        None
    };
    let add_root = if can_edit {
        Some(add_step_form(
            cx,
            account,
            project,
            plan.id,
            0_i64,
            "Add step",
            editor.clone(),
        ))
    } else {
        None
    };
    let anchor_editor = if can_edit {
        Some(link_form(
            cx,
            account,
            project,
            plan.id,
            0,
            ("Set anchor issue", "anchor"),
            editor.clone(),
        ))
    } else {
        None
    };
    let clear_anchor = if can_edit && plan.issue_id.is_some() {
        Some(action_button(
            cx,
            account,
            project,
            plan.id,
            0,
            PlanAction {
                action: "anchor",
                value: "",
                label: "Clear anchor",
                class: "text-[var(--text-faint)] hover:text-[var(--error)]",
            },
            editor.clone(),
        ))
    } else {
        None
    };
    let delete = if can_edit {
        Some(action_button(
            cx,
            account,
            project,
            plan.id,
            0,
            PlanAction {
                action: "delete_plan",
                value: "",
                label: "Delete plan",
                class: "text-[var(--error)] hover:bg-[var(--bg-subtle)]",
            },
            editor.clone(),
        ))
    } else {
        None
    };
    let statuses = ["active", "done", "archived"]
        .into_iter()
        .map(|next| {
            let class = if next == status {
                "text-[var(--accent)] bg-[var(--accent-subtle)]"
            } else {
                "text-[var(--text)] hover:bg-[var(--bg-subtle)]"
            };
            if can_edit {
                Some(action_button(
                    cx,
                    account,
                    project,
                    plan.id,
                    0_i64,
                    PlanAction {
                        action: "status",
                        value: next,
                        label: next,
                        class,
                    },
                    editor.clone(),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <main
            data-native-plan-detail=(identifier.clone())
            class="h-full overflow-y-auto"
        >
            <div
                class="max-w-[1080px] mx-auto px-6 py-6 grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_240px] gap-8"
            >
                <section class="min-w-0">
                    <a
                        class="text-caption text-[var(--text-muted)] hover:text-[var(--text)] no-underline"
                        (back)
                    >
                        "‹ Plans"
                    </a>
                    <div class="flex items-center gap-2 mt-3 mb-6">
                        <span class="text-caption font-mono text-[var(--text-faint)]">
                            (identifier)
                        </span>
                    </div>
                    if let Some(editor) = title_editor {
                        (editor)
                    } else {
                        <h1 class="text-heading font-medium text-[var(--text)]">
                            (title)
                        </h1>
                    }
                    <div class="flex flex-col gap-0.5 mt-5">
                        for step in steps {
                            (step)
                        }
                    </div>
                    if let Some(add) = add_root {
                        (add)
                    }
                    (super::super::issue_edit::activity::timeline(cx, activity))
                </section>
                <aside class="text-body-sm text-[var(--text-muted)]">
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Status"</p>
                        <div class="relative flex flex-col gap-1">
                            for status in statuses {
                                if let Some(status) = status {
                                    (status)
                                }
                            }
                        </div>
                    </div>
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Progress"</p>
                        <div class="flex items-center gap-2">
                            <div
                                class="flex-1 h-1.5 rounded-full bg-[var(--bg-subtle)] overflow-hidden"
                            >
                                <div
                                    class="h-full bg-[var(--accent)] rounded-full transition-all"
                                    style=(progress_width)
                                ></div>
                            </div>
                            <span
                                class="text-caption text-[var(--text-muted)] tabular-nums"
                            >
                                (progress_text)
                            </span>
                        </div>
                    </div>
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Anchor issue"</p>
                        if let Some(anchor) = anchor.as_deref() {
                            if let Some(href) = anchor_href {
                                <a
                                    class="font-mono text-[var(--accent)] hover:underline inline-flex items-center gap-1"
                                    (href)
                                >
                                    <span>(anchor.to_owned())</span>
                                    <span aria-hidden="true">"↗"</span>
                                </a>
                            }
                        } else {
                            <span>"None"</span>
                        }
                        if let Some(editor) = anchor_editor {
                            (editor)
                        }
                        if let Some(button) = clear_anchor {
                            (button)
                        }
                    </div>
                    <div class="issue-meta-dates py-3">
                        <div class="issue-meta-field mb-3">
                            <p class="issue-meta-field-label">"Created"</p>
                            <p class="m-0">
                                (super::super::dates::absolute_time_view(cx, &dates.0))
                            </p>
                        </div>
                        <div class="issue-meta-field">
                            <p class="issue-meta-field-label">"Updated"</p>
                            <p class="m-0">
                                (super::super::dates::absolute_time_view(cx, &dates.1))
                            </p>
                        </div>
                    </div>
                    if let Some(button) = delete {
                        <div class="border-t border-[var(--border)] pt-3">(button)</div>
                    }
                </aside>
            </div>
        </main>
    }.boxed()
}

fn prepare_step_expansion_state(cx: &Cx, steps: &[PlanStepNode]) {
    for step in steps {
        let row_cx = step_context(cx, step.id);
        let _collapsed = collapsed_state(&row_cx);
        prepare_step_expansion_state(&row_cx, &step.children);
    }
}

fn step_context(cx: &Cx, id: i64) -> Cx {
    cx.keyed(id)
}

fn collapsed_state(cx: &Cx) -> Signal<bool> {
    signal(cx, || false)
}

#[allow(clippy::too_many_arguments)]
fn step_node<'a>(
    cx: &Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    step: PlanStepNode,
    can_edit: bool,
    depth: usize,
    editor: PlanEditor,
) -> BoxView<'a> {
    let row_cx = step_context(cx, step.id);
    let collapsed = collapsed_state(&row_cx);
    let project = project.to_owned();
    let step_id = step.id;
    let title = step.title.clone();
    let is_collapsed = collapsed.get();
    let description = if is_collapsed || step.description.trim().is_empty() {
        None
    } else {
        Some(super::super::markdown::render(
            &row_cx,
            &step.description,
            super::super::markdown::Scope::Private,
            &[],
        ))
    };
    let issue_identifier = step.issue_identifier.clone();
    let issue_status = step.issue_status.clone().unwrap_or_else(|| "?".to_owned());
    let issue_href = issue_identifier
        .as_ref()
        .map(|identifier| issue_href(&row_cx, &project, identifier));
    let done = step.done;
    let children = if is_collapsed {
        Vec::new()
    } else {
        step.children
            .into_iter()
            .map(|child| {
                step_node(
                    &row_cx,
                    account,
                    &project,
                    plan_id,
                    child,
                    can_edit,
                    depth + 1,
                    editor.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let expansion = expansion_button(
        &row_cx,
        is_collapsed,
        collapsed,
        editor.revision.clone(),
        editor.busy.clone(),
    );
    let title_editor = if can_edit {
        Some(step_title_form(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            &title,
            editor.clone(),
        ))
    } else {
        None
    };
    let description_editor = if can_edit && !is_collapsed {
        Some(description_form(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            step.description,
            editor.clone(),
        ))
    } else {
        None
    };
    let link_editor = if can_edit && issue_identifier.is_none() {
        Some(link_form(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            ("Link an issue", "link"),
            editor.clone(),
        ))
    } else {
        None
    };
    let unlink = if can_edit && issue_identifier.is_some() {
        Some(action_button(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            PlanAction {
                action: "unlink",
                value: "",
                label: "Detach issue",
                class: "text-[var(--text-faint)] hover:text-[var(--text)]",
            },
            editor.clone(),
        ))
    } else {
        None
    };
    let remove = if can_edit {
        Some(action_button(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            PlanAction {
                action: "delete_step",
                value: "",
                label: "Delete step",
                class: "text-[var(--text-faint)] hover:text-[var(--error)]",
            },
            editor.clone(),
        ))
    } else {
        None
    };
    let toggle = if can_edit {
        Some(action_button(
            &row_cx,
            account,
            &project,
            plan_id,
            step_id,
            PlanAction {
                action: "done",
                value: if done { "false" } else { "true" },
                label: if done { "✓" } else { "○" },
                class: "size-4 shrink-0 rounded border flex items-center justify-center",
            },
            editor.clone(),
        ))
    } else {
        None
    };
    let padding = depth;
    OkBox::render(
        &row_cx,
        step_id,
        title,
        description,
        issue_identifier,
        issue_status,
        issue_href,
        done,
        expansion,
        children,
        title_editor,
        description_editor,
        link_editor,
        unlink,
        remove,
        toggle,
        padding,
    )
}

fn issue_href(cx: &Cx, fallback_project: &str, identifier: &str) -> Attributes {
    let project = identifier
        .rsplit_once('-')
        .map_or(fallback_project, |(prefix, _)| prefix);
    let mut attributes = navigation::attrs(cx, &format!("/{project}/issues/{identifier}"));
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        super::super::issue_peek::request_handler(identifier, true),
    );
    attributes
}

fn expansion_button<'a>(
    cx: &Cx,
    collapsed: bool,
    collapsed_state: Signal<bool>,
    revision: Signal<i64>,
    busy: Signal<bool>,
) -> BoxView<'a> {
    let label = if collapsed { "Expand" } else { "Collapse" };
    let icon_class = if collapsed {
        "transition-transform"
    } else {
        "rotate-90 transition-transform"
    };
    let refresh = revision;
    let action_busy = busy.clone();
    let browser = super::super::browser::bindings();
    let handler = expr!(|_event: Event| {
        if browser.is_disposed() {
            return;
        }
        if !action_busy.get() {
            collapsed_state.set(!collapsed_state.get());
            refresh.set(refresh.get() + 1_i64);
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    let icon_cx = cx.clone();
    view! {
        cx =>
        <button
            type="button"
            class="mt-0.5 size-4 shrink-0 flex items-center justify-center text-[var(--text-faint)] hover:text-[var(--text)]"
            aria-label=(label)
            aria-expanded=(if collapsed { "false" } else { "true" })
            title=(label)
            :disabled=$(busy.get())
            (attributes)
        >
            <span class=(icon_class)>
                (super::super::icons::ui_icon(
                    &icon_cx,
                    super::super::icons::UiIcon::Next,
                    12,
                ))
            </span>
        </button>
    }
    .boxed()
}

struct OkBox;

impl OkBox {
    #[allow(clippy::too_many_arguments)]
    fn render<'a>(
        cx: &Cx,
        step_id: i64,
        title: String,
        description: Option<String>,
        issue_identifier: Option<String>,
        issue_status: String,
        issue_href: Option<Attributes>,
        done: bool,
        expansion: BoxView<'a>,
        children: Vec<BoxView<'a>>,
        title_editor: Option<BoxView<'a>>,
        description_editor: Option<BoxView<'a>>,
        link_editor: Option<BoxView<'a>>,
        unlink: Option<BoxView<'a>>,
        remove: Option<BoxView<'a>>,
        toggle: Option<BoxView<'a>>,
        padding: usize,
    ) -> BoxView<'a> {
        let title_class = if done {
            "text-left text-body text-[var(--text-faint)] line-through"
        } else {
            "text-left text-body text-[var(--text)]"
        };
        view! {
            cx =>
            <article
                id=(format!("native-plan-step-{step_id}"))
                class="group flex flex-col py-1 rounded-md hover:bg-[var(--bg-subtle)]"
                data-plan-step=(step_id.to_string())
            >
                <div
                    class="flex items-start gap-2"
                    style=(format!("padding-left: min({}rem, 25%)", padding as f64 * 1.5))
                >
                    (expansion)
                    if let Some(toggle) = toggle {
                        (toggle)
                    } else {
                        <span
                            class="mt-0.5 size-4 shrink-0 rounded border flex items-center justify-center"
                            aria-label=(if done { "Done" } else { "Not done" })
                        >
                            {
                                if done {
                                    "✓"
                                } else {
                                    "○"
                                }
                            }
                        </span>
                    }
                    <div class="flex-1 min-w-0">
                        if let Some(editor) = title_editor {
                            <div>
                                <span class=(title_class)>(title.clone())</span>
                                (editor)
                            </div>
                        } else {
                            <span class=(title_class)>(title.clone())</span>
                        }
                        if let Some(identifier) = issue_identifier {
                            if let Some(href) = issue_href {
                                <a
                                    class="inline-flex items-center gap-1 mt-1 text-micro font-mono text-[var(--accent)] hover:underline no-underline"
                                    (href)
                                >
                                    (format!("{}: {}", identifier, issue_status))
                                    <span aria-hidden="true">"↗"</span>
                                </a>
                            }
                        }
                        if let Some(body) = description {
                            <div class="prose-step mt-1 text-body-sm">
                                (Unescaped::new_unchecked(body))
                            </div>
                        }
                        if let Some(editor) = description_editor {
                            (editor)
                        }
                        if let Some(editor) = link_editor {
                            (editor)
                        }
                        if let Some(unlink) = unlink {
                            (unlink)
                        }
                        if let Some(remove) = remove {
                            (remove)
                        }
                    </div>
                </div>
                for child in children {
                    (child)
                }
            </article>
        }.boxed()
    }
}

fn title_form<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    title: Signal<String>,
    editor: PlanEditor,
) -> BoxView<'a> {
    let draft = editor.title_draft;
    let editing = editor.title_editing;
    let revision = editor.revision;
    let message = editor.message;
    let project = project.to_owned();
    let browser = browser::bindings();
    let start_draft = draft.clone();
    let start_editing = editing.clone();
    let start_message = message.clone();
    let start = expr!(|| {
        if !browser.is_disposed() {
            start_draft.set(title.get());
            start_message.set("".to_owned());
            start_editing.set(true);
        }
    });
    let start_key = title_activation_handler(&start);

    let submit_draft = draft.clone();
    let submit_editing = editing.clone();
    let submit_revision = revision.clone();
    let submit_message = message.clone();
    let failed_message = message.clone();
    let failed_message_async = failed_message.clone();
    let submit_revision_async = submit_revision.clone();
    let saved_title = title.clone();
    let commit = expr!(|| {
        if !browser.is_disposed() {
            if submit_editing.get() {
                submit_editing.set(false);
                let next_title = submit_draft.get().trim_ecmascript();
                if !next_title.is_empty() {
                    if next_title != saved_title.get() {
                        submit_message.set("".to_owned());
                        let _failed = || {
                            if !browser.is_disposed() {
                                failed_message_async.set("Unable to save plan title.".to_owned());
                            }
                        };
                        let _run = async || {
                            let committed_title = mutate_plan(
                                account,
                                project,
                                plan_id,
                                0_i64,
                                "title".to_owned(),
                                next_title,
                            )
                            .await;
                            if !browser.is_disposed() {
                                saved_title.set(committed_title);
                                submit_revision_async.set(submit_revision_async.get() + 1_i64);
                            }
                        };
                        raw!(
                            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                            ()
                        );
                    }
                }
            }
        }
    });
    let cancel_draft = draft.clone();
    let cancel_editing = editing.clone();
    let cancel = expr!(|| {
        if !browser.is_disposed() {
            if cancel_editing.get() {
                cancel_draft.set(title.get());
                cancel_editing.set(false);
            }
        }
    });
    let finish_key = title_finish_handler(editing.clone(), &commit, &cancel, true);
    let finish_blur = title_finish_handler(editing.clone(), &commit, &cancel, false);
    let input_draft = draft.clone();
    let input = expr!(|event: Event| {
        if !browser.is_disposed() {
            input_draft.set(event.target.value.to_owned());
        }
    });
    let mut trigger = Attributes::with_capacity(2);
    trigger.insert(cx, "data-native-plan-title-trigger", "true");
    trigger.insert(cx, "data-topcoat-on:click", title_callback_handler(&start));
    trigger.insert(cx, "data-topcoat-on:keydown", start_key);
    let mut editor_attributes = Attributes::with_capacity(3);
    editor_attributes.insert(cx, "data-native-plan-title-input", "true");
    editor_attributes.insert(cx, "data-topcoat-on:input", input.into_evaluated_and_js().1);
    editor_attributes.insert(cx, "data-topcoat-on:keydown", finish_key);
    editor_attributes.insert(cx, "data-topcoat-on:blur", finish_blur);
    view! {
        cx =>
        if editing.get() {
            <input
                type="text"
                class="w-full text-title mb-4 font-display tracking-tight bg-transparent border-0 border-b-2 border-solid border-b-[var(--accent)] outline-none text-[var(--text)] py-1"
                :value=$(draft.get())
                autofocus="autofocus"
                (editor_attributes)
            />
        } else {
            <button
                type="button"
                class="text-title mb-4 w-full text-left font-display tracking-tight text-[var(--text)] py-1 rounded transition-colors bg-transparent border-0 cursor-text hover:bg-[var(--bg-subtle)]"
                (trigger)
            >
                $(title.get())
            </button>
        }
    }.boxed()
}

fn title_callback_handler<C: FnOnce()>(callback: &Expr<C>) -> Js {
    let browser = browser::bindings();
    let factory = expr!(|_event: Event, callback: C| {
        if !browser.is_disposed() {
            browser.call0(callback);
        }
    });
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(callback)
        .source(")")
        .build()
}

fn title_activation_handler<C: FnOnce()>(callback: &Expr<C>) -> Js {
    let browser = browser::bindings();
    let factory = expr!(|event: Event, callback: C| {
        if if event.key == "Enter" {
            true
        } else {
            event.key == " "
        } {
            event.prevent_default();
            if !browser.is_disposed() {
                browser.call0(callback);
            }
        }
    });
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(callback)
        .source(")")
        .build()
}

fn title_finish_handler<C: FnOnce(), D: FnOnce()>(
    editing: Signal<bool>,
    commit: &Expr<C>,
    cancel: &Expr<D>,
    keyboard: bool,
) -> Js {
    let browser = browser::bindings();
    let factory = expr!(|event: Event, commit: C, cancel: D| {
        if !browser.is_disposed() {
            if editing.get() {
                if keyboard {
                    if event.key == "Escape" {
                        event.prevent_default();
                        browser.call0(cancel);
                    } else {
                        let save_shortcut = if event.key == "s" {
                            if event.ctrl_key { true } else { event.meta_key }
                        } else {
                            false
                        };
                        if event.key == "Enter" {
                            event.prevent_default();
                            browser.call0(commit);
                        } else if save_shortcut {
                            event.prevent_default();
                            browser.call0(commit);
                        }
                    }
                } else {
                    browser.call0(commit);
                }
            }
        }
    });
    Js::builder()
        .source("event => (")
        .expression(&factory)
        .source(")(event, ")
        .expression(commit)
        .source(", ")
        .expression(cancel)
        .source(")")
        .build()
}

fn description_form<'a>(
    cx: &Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    step_id: i64,
    description: String,
    editor: PlanEditor,
) -> BoxView<'a> {
    let project = project.to_owned();
    let revision = editor.revision;
    let target = editor.step_description_target;
    let draft = editor.step_description_draft;
    let busy = editor.busy;
    let message = editor.message;
    let edit_target = target.clone();
    let edit_draft = draft.clone();
    let saved_description = description;
    let submit_draft = draft.clone();
    let submit_target = target.clone();
    let submit_revision = revision;
    let submit_busy = busy.clone();
    let submit_message = message.clone();
    let failed_busy = busy.clone();
    let failed_message = message;
    let edit = expr!(|_event: Event| {
        if edit_target.get() != step_id {
            edit_target.set(step_id);
            edit_draft.set(saved_description.clone());
        }
    });
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let value = submit_draft.get();
        let submitted = value.clone();
        if !submit_busy.get() {
            submit_busy.set(true);
            submit_message.set("".to_owned());
            let _failed = || {
                failed_busy.set(false);
                failed_message.set("Unable to save step details.".to_owned());
            };
            let _run = async || {
                mutate_plan(
                    account,
                    project,
                    plan_id,
                    step_id,
                    "description".to_owned(),
                    value,
                )
                .await;
                submit_busy.set(false);
                if submit_target.get() == step_id {
                    if submit_draft.get() == submitted {
                        submit_target.set(0_i64);
                    }
                }
                submit_revision.set(submit_revision.get() + 1_i64);
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut submit = Attributes::with_capacity(1);
    submit.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    let mut edit_attrs = Attributes::with_capacity(1);
    edit_attrs.insert(cx, "data-topcoat-on:click", edit.into_evaluated_and_js().1);
    view! {
        cx =>
        <div class="mt-1">
            <button
                type="button"
                class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]"
                (edit_attrs)
            >
                "Edit details"
            </button>
            <form class="mt-2" (submit) :hidden=$(target.get() != step_id)>
                <textarea
                    class="w-full bg-transparent outline-none text-body-sm leading-relaxed text-[var(--text)] border border-[var(--border)] rounded-md p-2 resize-y min-h-[80px]"
                    :value=$(draft.get())
                    @input=$(move |event: Event| draft.set(event.target.value))
                    placeholder="Describe this step… (markdown supported)"
                ></textarea>
                <div class="flex items-center gap-2 mt-1">
                    <button
                        class="text-caption font-medium text-[var(--accent-text)] bg-[var(--accent)] px-2 py-1 rounded-md"
                        type="submit"
                        :disabled=$(busy.get())
                    >
                        "Save"
                    </button>
                </div>
            </form>
        </div>
    }.boxed()
}

fn step_title_form<'a>(
    cx: &Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    step_id: i64,
    title: &str,
    editor: PlanEditor,
) -> BoxView<'a> {
    let project = project.to_owned();
    let revision = editor.revision;
    let target = editor.step_title_target;
    let draft = editor.step_title_draft;
    let busy = editor.busy;
    let message = editor.message;
    let edit_target = target.clone();
    let edit_draft = draft.clone();
    let saved_title = title.to_owned();
    let submit_draft = draft.clone();
    let submit_target = target.clone();
    let submit_revision = revision;
    let submit_busy = busy.clone();
    let submit_message = message.clone();
    let failed_busy = busy.clone();
    let failed_message = message;
    let edit = expr!(|_event: Event| {
        if edit_target.get() != step_id {
            edit_target.set(step_id);
            edit_draft.set(saved_title.clone());
        }
    });
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let submitted = submit_draft.get();
        let value = submitted.trim_ecmascript();
        if !value.is_empty() {
            if !submit_busy.get() {
                submit_busy.set(true);
                submit_message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Unable to save step title.".to_owned());
                };
                let _run = async || {
                    mutate_plan(
                        account,
                        project,
                        plan_id,
                        step_id,
                        "step_title".to_owned(),
                        value,
                    )
                    .await;
                    submit_busy.set(false);
                    if submit_target.get() == step_id {
                        if submit_draft.get() == submitted {
                            submit_target.set(0_i64);
                        }
                    }
                    submit_revision.set(submit_revision.get() + 1_i64);
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut submit = Attributes::with_capacity(1);
    submit.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    let mut edit_attrs = Attributes::with_capacity(1);
    edit_attrs.insert(cx, "data-topcoat-on:click", edit.into_evaluated_and_js().1);
    view! {
        cx =>
        <div class="my-1">
            <button
                type="button"
                class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]"
                (edit_attrs)
            >
                "Edit title"
            </button>
            <form
                class="flex items-center gap-2 my-2"
                (submit)
                :hidden=$(target.get() != step_id)
            >
                <input
                    class="flex-1 bg-transparent outline-none text-body text-[var(--text)] border-b border-[var(--accent)]"
                    :value=$(draft.get())
                    @input=$(move |event: Event| draft.set(event.target.value))
                />
                <button
                    type="submit"
                    class="text-caption text-[var(--accent)] hover:underline"
                    :disabled=$(busy.get())
                >
                    "Save"
                </button>
            </form>
        </div>
    }.boxed()
}

fn add_step_form<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    parent_step_id: i64,
    label: &str,
    editor: PlanEditor,
) -> BoxView<'a> {
    let draft = signal(cx, String::new);
    let project = project.to_owned();
    let placeholder = format!("{label}…");
    let revision = editor.revision;
    let busy = editor.busy;
    let message = editor.message;
    let submit_draft = draft.clone();
    let submit_revision = revision;
    let submit_busy = busy.clone();
    let submit_message = message.clone();
    let failed_busy = busy.clone();
    let failed_message = message;
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let submitted = submit_draft.get();
        let title = submitted.trim_ecmascript();
        if !title.is_empty() {
            if !submit_busy.get() {
                submit_busy.set(true);
                submit_message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Unable to add plan step.".to_owned());
                };
                let _run = async || {
                    mutate_plan(
                        account,
                        project,
                        plan_id,
                        parent_step_id,
                        "add_step".to_owned(),
                        title,
                    )
                    .await;
                    submit_busy.set(false);
                    if submit_draft.get() == submitted {
                        submit_draft.set("".to_owned());
                    }
                    submit_revision.set(submit_revision.get() + 1_i64);
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut submit = Attributes::with_capacity(1);
    submit.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <form class="mt-3 flex items-center gap-2" (submit)>
            <input
                class="flex-1 bg-transparent outline-none text-body text-[var(--text)] border-b border-[var(--border)]"
                placeholder=(placeholder)
                :value=$(draft.get())
                @input=$(move |event: Event| draft.set(event.target.value))
            />
            <button
                class="text-body-sm text-[var(--accent)] hover:underline"
                type="submit"
                :disabled=$(busy.get())
            >
                "Add"
            </button>
        </form>
    }.boxed()
}

fn link_form<'a>(
    cx: &Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    target_id: i64,
    label_action: (&str, &str),
    editor: PlanEditor,
) -> BoxView<'a> {
    let draft = signal(cx, String::new);
    let project = project.to_owned();
    let (label, action) = label_action;
    let action = action.to_owned();
    let summary = format!("{label}…");
    let revision = editor.revision;
    let busy = editor.busy;
    let message = editor.message;
    let submit_draft = draft.clone();
    let submit_revision = revision;
    let submit_busy = busy.clone();
    let submit_message = message.clone();
    let failed_busy = busy.clone();
    let failed_message = message;
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let value = submit_draft.get().trim_ecmascript();
        if !value.is_empty() {
            if !submit_busy.get() {
                submit_busy.set(true);
                submit_message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Unable to link issue.".to_owned());
                };
                let _run = async || {
                    mutate_plan(account, project, plan_id, target_id, action, value).await;
                    submit_busy.set(false);
                    submit_revision.set(submit_revision.get() + 1_i64);
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut submit = Attributes::with_capacity(1);
    submit.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <details class="mt-2">
            <summary
                class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]"
            >
                (summary)
            </summary>
            <form class="mt-2 flex items-center gap-2" (submit)>
                <input
                    class="w-32 bg-transparent outline-none font-mono text-caption text-[var(--text)] border-b border-[var(--border)]"
                    placeholder="LIF-42"
                    :value=$(draft.get())
                    @input=$(move |event: Event| draft.set(event.target.value))
                />
                <button
                    class="text-caption text-[var(--accent)] hover:underline"
                    type="submit"
                    :disabled=$(busy.get())
                >
                    "Link"
                </button>
            </form>
        </details>
    }.boxed()
}

fn action_button<'a>(
    cx: &Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    target_id: i64,
    specification: PlanAction<'_>,
    editor: PlanEditor,
) -> BoxView<'a> {
    let PlanAction {
        action,
        value,
        label,
        class,
    } = specification;
    let project = project.to_owned();
    let action = action.to_owned();
    let value = value.to_owned();
    let destination = if action == "delete_plan" {
        transport::mounted_url(cx, &format!("/{project}/plans"))
    } else {
        transport::mounted_url(cx, &format!("/{project}/plans/{plan_id}"))
    };
    let label = label.to_owned();
    let class = class.to_owned();
    let revision = editor.revision;
    let busy = editor.busy;
    let message = editor.message;
    let delete_action = action == "delete_plan";
    let request_action = action.clone();
    let aria = if action == "done" {
        if value == "true" {
            "Mark step done"
        } else {
            "Mark step not done"
        }
    } else {
        label.as_str()
    }
    .to_owned();
    let (label, text_case) = if action == "unlink" {
        ("unlink".to_owned(), "")
    } else {
        (label, " capitalize")
    };
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let browser = super::super::browser::bindings();
    let handler = expr!(|_event: Event| {
        if browser.is_disposed() {
            return;
        }
        if !busy.get() {
            let confirmed = if delete_action {
                raw!(
                    "cx.hydrate(window.confirm('Delete this plan? This cannot be undone.'))",
                    false
                )
            } else {
                true
            };
            if confirmed {
                busy.set(true);
                message.set("".to_owned());
                let _failed = || {
                    if !browser.is_disposed() {
                        failed_busy.set(false);
                        failed_message.set("Unable to update plan.".to_owned());
                    }
                };
                let _run = async || {
                    if !browser.is_disposed() {
                        mutate_plan(account, project, plan_id, target_id, request_action, value)
                            .await;
                        if !browser.is_disposed() {
                            busy.set(false);
                            if delete_action {
                                browser.navigate(destination);
                            } else {
                                revision.set(revision.get() + 1_i64);
                            }
                        }
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attributes = Attributes::with_capacity(2);
    if action == "unlink" {
        attributes.insert(cx, "title", aria.clone());
    }
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <button
            type="button"
            aria-label=(aria)
            class=(format!(
                "w-full text-left px-2 py-1 rounded-md text-body-sm{text_case} {class}",
            ))
            (attributes)
        >
            (label)
        </button>
    }
    .boxed()
}

#[procedure("/__native_plans/mutate")]
async fn mutate_plan(
    cx: &Cx,
    account: i64,
    project: String,
    plan_id: i64,
    target_id: i64,
    action: String,
    value: String,
) -> topcoat::Result<String> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let plan = session::read(
        cx,
        crate::services::plans::get(context::db(cx), &caller.identity, plan_id),
    )?;
    let project_id = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            crate::db::queries::resolve_project_identifier(&conn, &project)
        })(),
    )?;
    if plan.project_id != project_id {
        return session::read(
            cx,
            Err(LificError::NotFound(format!("plan {plan_id} not found"))),
        );
    }
    let identity = caller.identity.clone();
    let db = context::db(cx).clone();
    let hub = app_context::<RealtimeHub>(cx).clone();
    session::read(
        cx,
        caller
            .scope(async move {
                match action.as_str() {
                    "title" => crate::services::plans::update(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        &UpdatePlan {
                            title: Some(value),
                            ..Default::default()
                        },
                    )
                    .map(|plan| plan.title),
                    "status" => crate::services::plans::update(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        &UpdatePlan {
                            status: Some(value),
                            ..Default::default()
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "anchor" => {
                        let issue_id = if value.is_empty() {
                            None
                        } else {
                            let conn = db.read()?;
                            Some(crate::db::queries::resolve_identifier(&conn, &value)?)
                        };
                        crate::services::plans::update(
                            &db,
                            &hub,
                            &identity,
                            plan_id,
                            &UpdatePlan {
                                issue_id: Some(issue_id),
                                ..Default::default()
                            },
                        )
                        .map(|_| "saved".to_owned())
                    }
                    "add_step" => crate::services::plans::add_step(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        &crate::services::plans::AddStep {
                            parent_step_id: if target_id == 0 {
                                None
                            } else {
                                Some(target_id)
                            },
                            title: value,
                            description: String::new(),
                            issue_id: None,
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "done" => crate::services::plans::update_step(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        target_id,
                        &crate::services::plans::UpdateStep {
                            done: Some(value == "true"),
                            ..Default::default()
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "step_title" => crate::services::plans::update_step(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        target_id,
                        &crate::services::plans::UpdateStep {
                            title: Some(value),
                            ..Default::default()
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "description" => crate::services::plans::update_step(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        target_id,
                        &crate::services::plans::UpdateStep {
                            description: Some(value),
                            ..Default::default()
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "link" => {
                        let conn = db.read()?;
                        let issue_id = crate::db::queries::resolve_identifier(&conn, &value)?;
                        crate::services::plans::update_step(
                            &db,
                            &hub,
                            &identity,
                            plan_id,
                            target_id,
                            &crate::services::plans::UpdateStep {
                                issue_id: Some(Some(issue_id)),
                                ..Default::default()
                            },
                        )
                        .map(|_| "saved".to_owned())
                    }
                    "unlink" => crate::services::plans::update_step(
                        &db,
                        &hub,
                        &identity,
                        plan_id,
                        target_id,
                        &crate::services::plans::UpdateStep {
                            issue_id: Some(None),
                            ..Default::default()
                        },
                    )
                    .map(|_| "saved".to_owned()),
                    "delete_step" => crate::services::plans::delete_step(
                        &db, &hub, &identity, plan_id, target_id,
                    )
                    .map(|_| "saved".to_owned()),
                    "delete_plan" => crate::services::plans::delete(&db, &hub, &identity, plan_id)
                        .map(|_| "deleted".to_owned()),
                    _ => Err(LificError::BadRequest("Unknown plan action.".into())),
                }
            })
            .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use topcoat::runtime::Surrogated;

    #[tokio::test]
    async fn native_plan_title_starts_as_text_and_emits_inline_edit_handlers() {
        use crate::db::{models::CreatePlan, queries};
        use scraper::{Html, Selector};

        let fixture = super::super::super::home_fixture::fixture();
        let (plan, account) = {
            let conn = fixture.db.write().unwrap();
            let account = queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id;
            let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
            queries::members::upsert_member(&conn, project_id, account, Role::Maintainer).unwrap();
            let plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id,
                    title: "Inline title parity".into(),
                    issue_id: None,
                    steps: Vec::new(),
                },
            )
            .unwrap();
            (plan, account)
        };

        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = Html::parse_document(&html);
        let trigger = document
            .select(&Selector::parse("button[data-native-plan-title-trigger]").unwrap())
            .next()
            .expect("editable plan title is initially rendered as a button");
        assert_eq!(trigger.text().collect::<String>(), "Inline title parity");
        assert!(
            document
                .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
                .next()
                .is_none(),
            "title input is only rendered while editing"
        );

        let result = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/title_handler.test.cjs",
            &serde_json::json!({
                "signals": super::super::super::home_fixture::page_signals(&html),
                "click_handler": trigger.value().attr("data-topcoat-on:click").unwrap(),
                "keydown_handler": trigger.value().attr("data-topcoat-on:keydown").unwrap(),
                "title": "Inline title parity",
            }),
        );
        let mut click_editing_html = String::new();
        for key in ["click_signals", "keyboard_signals", "space_signals"] {
            let signals = result[key]
                .as_object()
                .expect("runtime returns the full changed signal snapshot")
                .clone();
            let (status, render_html) = super::super::super::home_fixture::document(
                &fixture,
                "/app",
                &format!("/ACC/plans/{}", plan.id),
                true,
                Some(signals),
            )
            .await;
            assert_eq!(status, axum::http::StatusCode::OK);
            let editing_document = Html::parse_document(&render_html);
            let input = editing_document
                .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
                .next()
                .expect("click and keyboard activation render the title editor");
            assert_eq!(input.value().attr("value"), Some("Inline title parity"));
            assert!(
                editing_document
                    .select(&Selector::parse("button[data-native-plan-title-trigger]").unwrap())
                    .next()
                    .is_none()
            );
            if key == "click_signals" {
                click_editing_html = render_html;
            }
        }

        let editing_document = Html::parse_document(&click_editing_html);
        let input = editing_document
            .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
            .next()
            .unwrap();
        let signals = super::super::super::home_fixture::page_signals(&click_editing_html);
        let emitted = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/title_handler.test.cjs",
            &serde_json::json!({
                "signals": super::super::super::home_fixture::page_signals(&html),
                "edit_signals": signals,
                "click_handler": trigger.value().attr("data-topcoat-on:click").unwrap(),
                "keydown_handler": trigger.value().attr("data-topcoat-on:keydown").unwrap(),
                "input_handler": input.value().attr("data-topcoat-on:input").unwrap(),
                "edit_keydown_handler": input.value().attr("data-topcoat-on:keydown").unwrap(),
                "blur_handler": input.value().attr("data-topcoat-on:blur").unwrap(),
                "title": "Inline title parity",
                "response": serde_json::to_value("Renamed plan title".to_owned().into_surrogate()).unwrap(),
                "pending_response": serde_json::to_value("Saving title".to_owned().into_surrogate()).unwrap(),
            }),
        );
        for (key, saved_title) in [
            ("ctrl_save_request", "Saved by ctrl+s"),
            ("meta_save_request", "Saved by meta+s"),
            ("blur_save_request", "Saved by blur"),
        ] {
            assert_eq!(
                emitted[key]["arguments"],
                serde_json::to_value(
                    (
                        account,
                        "ACC".to_owned(),
                        plan.id,
                        0_i64,
                        "title".to_owned(),
                        saved_title.to_owned(),
                    )
                        .into_surrogate(),
                )
                .unwrap(),
                "{key} emits the trimmed title payload",
            );
        }
        assert_eq!(emitted["cancel_prevented"], true);
        for key in ["cancelled_signals", "empty_signals", "unchanged_signals"] {
            let (status, closed_html) = super::super::super::home_fixture::document(
                &fixture,
                "/app",
                &format!("/ACC/plans/{}", plan.id),
                true,
                Some(emitted[key].as_object().unwrap().clone()),
            )
            .await;
            assert_eq!(status, axum::http::StatusCode::OK);
            let closed_document = Html::parse_document(&closed_html);
            assert!(
                closed_document
                    .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
                    .next()
                    .is_none(),
                "Escape, empty and unchanged titles close the editor"
            );
            assert_eq!(
                closed_document
                    .select(&Selector::parse("button[data-native-plan-title-trigger]").unwrap())
                    .next()
                    .unwrap()
                    .text()
                    .collect::<String>(),
                "Inline title parity",
            );
        }
        let (status, failed_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(emitted["failed_signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let failed_document = Html::parse_document(&failed_html);
        let alert = failed_document
            .select(&Selector::parse("[data-native-plan-owner] > p[role='alert']").unwrap())
            .next()
            .expect("plan owner renders the title save error");
        assert_eq!(
            alert.text().collect::<String>(),
            "Unable to save plan title."
        );
        let arguments = emitted["request"]["arguments"].clone();
        let expected_arguments = serde_json::to_value(
            (
                account,
                "ACC".to_owned(),
                plan.id,
                0_i64,
                "title".to_owned(),
                "Renamed plan title".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap();
        assert_eq!(arguments, expected_arguments);
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let updated = queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id).unwrap();
        assert_eq!(updated.title, "Renamed plan title");
        assert!(fixture
            .db
            .read()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM audit_log WHERE entity_type = 'plan' AND entity_id = ?1 AND field = 'title' AND new_value = 'Renamed plan title')",
                [plan.id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap());
        let (status, saved_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(emitted["commit_signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let saved_document = Html::parse_document(&saved_html);
        let saved_title = saved_document
            .select(&Selector::parse("button[data-native-plan-title-trigger]").unwrap())
            .next()
            .expect("successful revision rerender uses canonical saved title");
        assert_eq!(saved_title.text().collect::<String>(), "Renamed plan title");

        let pending_arguments = emitted["pending_request"]["arguments"].clone();
        let expected_pending_arguments = serde_json::to_value(
            (
                account,
                "ACC".to_owned(),
                plan.id,
                0_i64,
                "title".to_owned(),
                "Saving title".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap();
        assert_eq!(pending_arguments, expected_pending_arguments);
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            pending_arguments,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let (status, pending_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(emitted["pending_signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let pending_document = Html::parse_document(&pending_html);
        let pending_input = pending_document
            .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
            .next()
            .expect("saving an earlier title preserves a newer editing session");
        assert_eq!(
            pending_input.value().attr("value"),
            Some("Newer unsaved draft")
        );
        let close = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/title_handler.test.cjs",
            &serde_json::json!({
                "mode": "close",
                "signals": super::super::super::home_fixture::page_signals(&html),
                "edit_signals": super::super::super::home_fixture::page_signals(&pending_html),
                "click_handler": trigger.value().attr("data-topcoat-on:click").unwrap(),
                "keydown_handler": trigger.value().attr("data-topcoat-on:keydown").unwrap(),
                "input_handler": pending_input.value().attr("data-topcoat-on:input").unwrap(),
                "edit_keydown_handler": pending_input.value().attr("data-topcoat-on:keydown").unwrap(),
                "blur_handler": pending_input.value().attr("data-topcoat-on:blur").unwrap(),
            }),
        );
        let (status, closed_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(close["close_signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let closed_document = Html::parse_document(&closed_html);
        let canonical_trigger = closed_document
            .select(&Selector::parse("button[data-native-plan-title-trigger]").unwrap())
            .next()
            .unwrap();
        assert_eq!(canonical_trigger.text().collect::<String>(), "Saving title");
        let reopened = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/title_handler.test.cjs",
            &serde_json::json!({
                "signals": super::super::super::home_fixture::page_signals(&closed_html),
                "click_handler": canonical_trigger.value().attr("data-topcoat-on:click").unwrap(),
                "keydown_handler": canonical_trigger.value().attr("data-topcoat-on:keydown").unwrap(),
            }),
        );
        let (status, reopened_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(reopened["click_signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let reopened_document = Html::parse_document(&reopened_html);
        assert_eq!(
            reopened_document
                .select(&Selector::parse("input[data-native-plan-title-input]").unwrap())
                .next()
                .unwrap()
                .value()
                .attr("value"),
            Some("Saving title"),
            "a subsequent edit starts from the canonical saved title",
        );
    }

    #[component]
    async fn step_render_fixture(cx: &Cx, step: PlanStepNode) -> topcoat::Result<impl View> {
        let rendered = step_node(
            cx,
            1,
            "LIF",
            1,
            step,
            false,
            2,
            PlanEditor {
                revision: signal(cx, || 0_i64),
                canonical_title: signal(cx, String::new),
                title_draft: signal(cx, String::new),
                title_editing: signal(cx, || false),
                step_title_target: signal(cx, || 0_i64),
                step_title_draft: signal(cx, String::new),
                step_description_target: signal(cx, || 0_i64),
                step_description_draft: signal(cx, String::new),
                busy: signal(cx, || false),
                message: signal(cx, String::new),
            },
        );
        Ok(view! { cx => (rendered) })
    }

    #[tokio::test]
    async fn detail_step_links_issue_and_renders_sanitized_markdown() {
        let cx = Cx::default();
        let step = PlanStepNode {
            id: 1,
            plan_id: 1,
            parent_step_id: None,
            position: 0,
            title: "Build feature".into(),
            description: "Details **matter**".into(),
            issue_id: Some(2),
            issue_identifier: Some("OTHER-TEAM-42".into()),
            issue_status: Some("todo".into()),
            done: true,
            reopened_via_issue_at: None,
            created_at: String::new(),
            edited_at: None,
            children: Vec::new(),
        };
        let html = view! { cx => step_render_fixture(step: step) }
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("Build feature"));
        assert!(html.contains("/OTHER-TEAM/issues/OTHER-TEAM-42"));
        assert!(html.contains("padding-left: min(3rem, 25%)"));
        assert!(html.contains("✓"));
        assert!(html.contains("<strong>matter</strong>"));
    }

    #[tokio::test]
    async fn production_plan_issue_links_emit_shift_peek_handlers() {
        use crate::db::{models::CreatePlan, queries};
        use scraper::{Html, Selector};

        let fixture = super::super::super::home_fixture::fixture();
        let (plan, issue_identifier) = {
            let conn = fixture.db.write().unwrap();
            let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
            let issue = queries::get_issue(&conn, issue_id).unwrap();
            let plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id: issue.project_id,
                    title: "Peek-linked plan".into(),
                    issue_id: Some(issue_id),
                    steps: Vec::new(),
                },
            )
            .unwrap();
            queries::plans::add_step(&conn, plan.id, None, "Linked step", "", Some(issue_id))
                .unwrap();
            (plan, issue.identifier)
        };

        let path = format!("/ACC/plans/{}", plan.id);
        let (status, html) =
            super::super::super::home_fixture::document(&fixture, "/app", &path, true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = Html::parse_document(&html);
        let links = document
            .select(&Selector::parse(&format!("a[href*=\"/issues/{issue_identifier}\"]")).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            links.len(),
            2,
            "plan and step issue links are both rendered"
        );
        let cases = links
            .iter()
            .map(|link| {
                serde_json::json!({
                    "href": link.value().attr("href").unwrap(),
                    "handler": link.value().attr("data-topcoat-on:click").unwrap(),
                })
            })
            .collect::<Vec<_>>();
        let result = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/issue_peek_handler.test.cjs",
            &serde_json::json!({
                "signals": super::super::super::home_fixture::page_signals(&html),
                "links": cases,
                "identifier": issue_identifier,
            }),
        );
        assert_eq!(
            result["peek_events"],
            serde_json::json!([
                {"type":"lific:native-issue-peek-request", "identifier":issue_identifier},
                {"type":"lific:native-issue-peek-request", "identifier":issue_identifier},
            ])
        );
        assert_eq!(result["normal_clicks"], 2);
    }

    #[tokio::test]
    async fn native_plan_detail_renders_the_latest_hundred_activity_rows_and_emitted_toggles() {
        use crate::db::{models::CreatePlan, queries};
        use scraper::{Html, Selector};

        let fixture = super::super::super::home_fixture::fixture();
        let (
            plan,
            foreign_plan,
            account,
            project_id,
            oldest_id,
            next_oldest_id,
            description_id,
            newest_id,
        ) = {
            let conn = fixture.db.write().unwrap();
            let account = queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id;
            let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
            let plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id,
                    title: "Activity integration plan".into(),
                    issue_id: None,
                    steps: Vec::new(),
                },
            )
            .unwrap();
            let foreign_plan = queries::plans::create_plan(
                &conn,
                &CreatePlan {
                    project_id,
                    title: "Different plan scope".into(),
                    issue_id: None,
                    steps: Vec::new(),
                },
            )
            .unwrap();

            // The Plan feed contains this sentinel plus the plan's create row.
            // A 100-row limit must omit sentinel zero and retain the newest 100.
            for index in 0..101 {
                let description = index == 98;
                let entity_type = if description { "plan_step" } else { "plan" };
                let entity_id = if description { 50_000 + index } else { plan.id };
                let field = if description { "description" } else { "title" };
                let old = if description {
                    "Old description line\nunchanged context"
                } else {
                    "Old title"
                };
                let new = if description {
                    "New description line\nunchanged context".to_owned()
                } else {
                    format!("Audit new {index}")
                };
                conn.execute(
                    "INSERT INTO audit_log
                     (transport, entity_type, entity_id, entity_label, project_id,
                      action, field, old_value, new_value)
                     VALUES ('system', ?1, ?2, ?3, ?4, 'update', ?5, ?6, ?7)",
                    rusqlite::params![
                        entity_type,
                        entity_id,
                        plan.identifier,
                        project_id,
                        field,
                        old,
                        new,
                    ],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO audit_log
                 (transport, entity_type, entity_id, entity_label, project_id,
                  action, field, old_value, new_value)
                 VALUES ('system', 'plan', ?1, ?2, ?3, 'update', 'title',
                         'Foreign old title', 'Foreign scope sentinel')",
                rusqlite::params![foreign_plan.id, foreign_plan.identifier, project_id],
            )
            .unwrap();
            let audit_id = |value: &str| {
                conn.query_row(
                    "SELECT id FROM audit_log WHERE new_value = ?1",
                    [value],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
            };
            (
                plan,
                foreign_plan,
                account,
                project_id,
                audit_id("Audit new 0"),
                audit_id("Audit new 1"),
                audit_id("New description line\nunchanged context"),
                audit_id("Audit new 100"),
            )
        };

        let path = format!("/ACC/plans/{}", plan.id);
        let (status, html) =
            super::super::super::home_fixture::document(&fixture, "/app", &path, true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = Html::parse_document(&html);
        let timeline = document
            .select(&Selector::parse("[data-native-issue-activity]").unwrap())
            .next()
            .expect("Plan detail includes the shared activity timeline");
        assert_eq!(
            timeline
                .select(&Selector::parse(".native-issue-activity__count").unwrap())
                .next()
                .unwrap()
                .text()
                .collect::<String>(),
            "100",
            "Plan detail matches Main's default 100-entry feed cap",
        );
        let rows = Selector::parse("li").unwrap();
        let row_text = timeline
            .select(&rows)
            .map(|row| row.text().collect::<String>())
            .collect::<Vec<_>>();
        assert_eq!(row_text.len(), 100);
        assert_eq!(
            timeline
                .select(&Selector::parse("li:not([hidden])").unwrap())
                .count(),
            6,
            "only the six most recent entries start expanded in the feed",
        );
        let newest_selector =
            Selector::parse(&format!("li[data-activity-id=\"{newest_id}\"]")).unwrap();
        let retained_selector =
            Selector::parse(&format!("li[data-activity-id=\"{next_oldest_id}\"]")).unwrap();
        let excluded_selector =
            Selector::parse(&format!("li[data-activity-id=\"{oldest_id}\"]")).unwrap();
        let description_selector =
            Selector::parse(&format!("li[data-activity-id=\"{description_id}\"]")).unwrap();
        assert_eq!(
            timeline.select(&newest_selector).count(),
            1,
            "newest row is present"
        );
        assert_eq!(
            timeline
                .select(&rows)
                .next()
                .unwrap()
                .value()
                .attr("data-activity-id"),
            Some(newest_id.to_string().as_str()),
            "entries are newest first"
        );
        assert_eq!(
            timeline.select(&retained_selector).count(),
            1,
            "next-oldest row is retained"
        );
        assert_eq!(
            timeline.select(&excluded_selector).count(),
            0,
            "oldest row is outside the 100-row cap"
        );
        let values_selector = Selector::parse(".native-issue-activity__values").unwrap();
        let initial_description = timeline.select(&description_selector).next().unwrap();
        let initial_values = initial_description.select(&values_selector).next().unwrap();
        assert!(
            initial_values.value().attr("hidden").is_some(),
            "description values start collapsed"
        );
        assert!(!html.contains("Foreign scope sentinel"));

        let button = Selector::parse(r"button[data-topcoat-on\:click]").unwrap();
        let mut show_all = None;
        let mut show_change = None;
        for control in timeline.select(&button) {
            let label = control.text().collect::<String>();
            let handler = control.value().attr("data-topcoat-on:click").unwrap();
            if label.contains("Show all 100 entries") {
                show_all = Some(handler);
            }
            if label.contains("show change") {
                show_change = Some(handler);
            }
        }
        let emitted = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/activity_handler.test.cjs",
            &serde_json::json!({
                "signals": super::super::super::home_fixture::page_signals(&html),
                "show_all": show_all.expect("Show all uses an emitted handler"),
                "show_change": show_change.expect("description expansion uses an emitted handler"),
            }),
        );
        let recovered = serde_json::from_value(emitted["signals"].clone()).unwrap();
        let (status, restored_html) = super::super::super::home_fixture::document(
            &fixture,
            "/app",
            &path,
            true,
            Some(recovered),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let restored = Html::parse_document(&restored_html);
        let restored_timeline = restored
            .select(&Selector::parse("[data-native-issue-activity]").unwrap())
            .next()
            .unwrap();
        assert_eq!(restored_timeline.select(&rows).count(), 100);
        assert_eq!(
            restored_timeline
                .select(&Selector::parse("li:not([hidden])").unwrap())
                .count(),
            100,
            "the emitted Show all handler reveals the remaining history",
        );
        let restored_description = restored_timeline
            .select(&description_selector)
            .next()
            .unwrap();
        let restored_values = restored_description
            .select(&values_selector)
            .next()
            .unwrap();
        assert!(
            restored_values.value().attr("hidden").is_none(),
            "emitted change handler reveals values"
        );
        let restored_text = restored_values.text().collect::<String>();
        assert!(restored_text.contains("Old description line"));
        assert!(restored_text.contains("New description line"));
        assert!(!restored_html.contains("Foreign scope sentinel"));

        // The fixture user is a viewer and can read the mounted Plan page.
        fixture
            .db
            .write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2",
                rusqlite::params![project_id, account],
            )
            .unwrap();
        let (revoked_status, _) =
            super::super::super::home_fixture::document(&fixture, "/app", &path, true, None).await;
        assert_eq!(revoked_status, axum::http::StatusCode::FORBIDDEN);
        assert_ne!(plan.id, foreign_plan.id);
    }

    #[tokio::test]
    async fn native_plan_step_tree_renders_collapse_control_for_viewers() {
        let fixture = super::super::super::home_fixture::fixture();
        let plan = {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id,
                    title: "Nested checklist".into(),
                    issue_id: None,
                    steps: vec![
                        crate::db::models::CreatePlanStep {
                            title: "Parent step".into(),
                            description: "Parent detail body".into(),
                            issue_id: None,
                            done: false,
                            steps: vec![crate::db::models::CreatePlanStep {
                                title: "Nested child".into(),
                                description: String::new(),
                                issue_id: None,
                                done: false,
                                steps: vec![],
                            }],
                        },
                        crate::db::models::CreatePlanStep {
                            title: "Other parent".into(),
                            description: String::new(),
                            issue_id: None,
                            done: false,
                            steps: vec![crate::db::models::CreatePlanStep {
                                title: "Other nested child".into(),
                                description: String::new(),
                                issue_id: None,
                                done: false,
                                steps: vec![],
                            }],
                        },
                    ],
                },
            )
            .unwrap()
        };
        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let parent = document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("parent step rendered");
        let collapse = parent
            .select(&scraper::Selector::parse("button[title='Collapse']").unwrap())
            .next()
            .expect("viewers can collapse expanded plan steps");
        assert_eq!(collapse.value().attr("aria-label"), Some("Collapse"));
        assert!(
            document
                .root_element()
                .text()
                .any(|text| text == "Parent detail body")
        );
        assert!(
            document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_some(),
            "nested steps are expanded by default",
        );
        assert!(
            document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[1].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_some(),
            "other parents start expanded too",
        );

        let collapse_handler = collapse
            .value()
            .attr("data-topcoat-on:click")
            .expect("collapse is wired to the native runtime");
        let collapsed = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": collapse_handler,
                "signals": super::super::super::home_fixture::page_signals(&html),
            }),
        );
        let collapsed_signals =
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
                collapsed["signals"].clone(),
            )
            .unwrap();
        let (status, collapsed_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(collapsed_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let collapsed_document = scraper::Html::parse_document(&collapsed_html);
        assert!(
            collapsed_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_none(),
            "collapsing a parent hides its descendants",
        );
        assert!(
            collapsed_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[1].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_some(),
            "collapsing one parent leaves other parents expanded",
        );
        let other_parent = collapsed_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[1].id
                ))
                .unwrap(),
            )
            .next()
            .unwrap();
        assert!(
            other_parent
                .select(&scraper::Selector::parse("button[title='Collapse']").unwrap())
                .next()
                .is_some()
        );
        let parent = collapsed_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[0].id
                ))
                .unwrap(),
            )
            .next()
            .unwrap();
        assert!(
            !parent.text().any(|text| text == "Parent detail body"),
            "collapsed steps hide their descriptions",
        );
        assert!(
            parent
                .select(
                    &scraper::Selector::parse(
                        "button[title='Expand'][aria-label='Expand'][aria-expanded='false']"
                    )
                    .unwrap()
                )
                .next()
                .is_some(),
            "collapsed steps offer the matching expand action",
        );

        let expand = parent
            .select(&scraper::Selector::parse("button[title='Expand']").unwrap())
            .next()
            .unwrap();
        let expanded = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": expand.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&collapsed_html),
            }),
        );
        let expanded_signals =
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
                expanded["signals"].clone(),
            )
            .unwrap();
        let (status, expanded_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            Some(expanded_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let expanded_document = scraper::Html::parse_document(&expanded_html);
        assert!(
            expanded_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_some(),
            "expanding the parent restores its descendants",
        );
        assert!(
            expanded_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[1].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_some(),
            "other parents retain their expanded state",
        );
        assert!(
            expanded_document
                .root_element()
                .text()
                .any(|text| text == "Parent detail body")
        );
    }

    #[tokio::test]
    async fn native_plan_step_expansion_is_ignored_while_plan_action_is_busy() {
        let fixture = super::super::super::home_fixture::fixture();
        let plan = {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id,
                    title: "Busy expansion".into(),
                    issue_id: None,
                    steps: vec![crate::db::models::CreatePlanStep {
                        title: "Parent step".into(),
                        description: String::new(),
                        issue_id: None,
                        done: false,
                        steps: vec![],
                    }],
                },
            )
            .unwrap()
        };
        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let collapse = document
            .select(&scraper::Selector::parse("button[title='Collapse']").unwrap())
            .next()
            .expect("step has a collapse control");
        let result = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": collapse.value().attr("data-topcoat-on:click").unwrap(),
                "mode": "busy",
                "signals": super::super::super::home_fixture::page_signals(&html),
            }),
        );
        assert!(result["requests"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn native_plan_step_collapse_state_survives_ancestor_toggle() {
        let fixture = super::super::super::home_fixture::fixture();
        let plan = {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id,
                    title: "Nested collapse state".into(),
                    issue_id: None,
                    steps: vec![crate::db::models::CreatePlanStep {
                        title: "Parent step".into(),
                        description: String::new(),
                        issue_id: None,
                        done: false,
                        steps: vec![crate::db::models::CreatePlanStep {
                            title: "Child step".into(),
                            description: String::new(),
                            issue_id: None,
                            done: false,
                            steps: vec![crate::db::models::CreatePlanStep {
                                title: "Grandchild step".into(),
                                description: String::new(),
                                issue_id: None,
                                done: false,
                                steps: vec![crate::db::models::CreatePlanStep {
                                    title: "Great-grandchild step".into(),
                                    description: String::new(),
                                    issue_id: None,
                                    done: false,
                                    steps: vec![],
                                }],
                            }],
                        }],
                    }],
                },
            )
            .unwrap()
        };
        let route = format!("/ACC/plans/{}", plan.id);
        let (status, html) =
            super::super::super::home_fixture::document(&fixture, "", &route, true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let child = document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}'] button[title='Collapse']",
                    plan.steps[0].children[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("child starts expanded");
        let collapsed_child = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": child.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&html),
            }),
        );
        let child_signals = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
            collapsed_child["signals"].clone(),
        )
        .unwrap();
        let (status, child_collapsed_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(child_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let child_collapsed_document = scraper::Html::parse_document(&child_collapsed_html);
        assert!(
            child_collapsed_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].children[0].id
                    ))
                    .unwrap()
                )
                .next()
                .is_none(),
            "collapsing the child hides its grandchild",
        );
        let parent = child_collapsed_document
            .select(&scraper::Selector::parse("button[title='Collapse']").unwrap())
            .next()
            .expect("ancestor remains expanded");
        let collapsed_parent = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": parent.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&child_collapsed_html),
            }),
        );
        let parent_signals = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
            collapsed_parent["signals"].clone(),
        )
        .unwrap();
        let (status, parent_collapsed_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(parent_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let parent_collapsed_document = scraper::Html::parse_document(&parent_collapsed_html);
        let parent = parent_collapsed_document
            .select(&scraper::Selector::parse("button[title='Expand']").unwrap())
            .next()
            .expect("ancestor is collapsed");
        let expanded_parent = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": parent.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&parent_collapsed_html),
            }),
        );
        let expanded_signals =
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
                expanded_parent["signals"].clone(),
            )
            .unwrap();
        let (status, expanded_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(expanded_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let expanded_document = scraper::Html::parse_document(&expanded_html);
        let child = expanded_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[0].children[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("child is visible after expanding ancestor");
        assert!(
            child
                .select(&scraper::Selector::parse("button[title='Expand']").unwrap())
                .next()
                .is_some(),
            "child retains its collapsed state after the ancestor hides it",
        );
        assert!(
            expanded_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].children[0].id
                    ))
                    .unwrap()
                )
                .next()
                .is_none(),
            "the collapsed child continues to hide its descendants",
        );

        let (status, initial_html) =
            super::super::super::home_fixture::document(&fixture, "", &route, true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let initial_document = scraper::Html::parse_document(&initial_html);
        let grandchild = initial_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}'] button[title='Collapse']",
                    plan.steps[0].children[0].children[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("grandchild starts expanded");
        let collapsed_grandchild = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": grandchild.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&initial_html),
            }),
        );
        let grandchild_signals =
            serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
                collapsed_grandchild["signals"].clone(),
            )
            .unwrap();
        let (status, grandchild_collapsed_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(grandchild_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let grandchild_collapsed_document =
            scraper::Html::parse_document(&grandchild_collapsed_html);
        assert!(
            grandchild_collapsed_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].children[0].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_none(),
            "collapsing the grandchild hides its great-grandchild",
        );
        let root_collapse = grandchild_collapsed_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}'] button[title='Collapse']",
                    plan.steps[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("root is still expanded");
        let collapsed_root = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": root_collapse.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&grandchild_collapsed_html),
            }),
        );
        let root_signals = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
            collapsed_root["signals"].clone(),
        )
        .unwrap();
        let (status, root_collapsed_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(root_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let root_collapsed_document = scraper::Html::parse_document(&root_collapsed_html);
        let root_expand = root_collapsed_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}'] button[title='Expand']",
                    plan.steps[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("root can expand again");
        let expanded_root = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/step_tree_handler.test.cjs",
            &serde_json::json!({
                "handler": root_expand.value().attr("data-topcoat-on:click").unwrap(),
                "signals": super::super::super::home_fixture::page_signals(&root_collapsed_html),
            }),
        );
        let root_expanded_signals = serde_json::from_value::<
            serde_json::Map<String, serde_json::Value>,
        >(expanded_root["signals"].clone())
        .unwrap();
        let (status, root_expanded_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &route,
            true,
            Some(root_expanded_signals),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let root_expanded_document = scraper::Html::parse_document(&root_expanded_html);
        let grandchild = root_expanded_document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[0].children[0].children[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("grandchild is visible after expanding root");
        assert!(
            grandchild
                .select(&scraper::Selector::parse("button[title='Expand']").unwrap())
                .next()
                .is_some(),
            "grandchild retains its collapsed state after the root hides it",
        );
        assert!(
            root_expanded_document
                .select(
                    &scraper::Selector::parse(&format!(
                        "article[data-plan-step='{}']",
                        plan.steps[0].children[0].children[0].children[0].id
                    ))
                    .unwrap(),
                )
                .next()
                .is_none(),
            "the collapsed grandchild still hides its great-grandchild",
        );
    }

    #[tokio::test]
    async fn native_plan_step_issue_unlink_uses_production_mutation_and_preserves_state() {
        let fixture = super::super::super::home_fixture::fixture();
        let (plan, account, issue_id) = {
            let conn = fixture.db.write().unwrap();
            let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id;
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            let issue_id = crate::db::queries::resolve_identifier(&conn, "ACC-1").unwrap();
            // This deliberately leaves a linked incomplete step beside a
            // completed issue. Unlink must preserve both independent states.
            conn.execute(
                "UPDATE issues SET status = 'done' WHERE id = ?1",
                [issue_id],
            )
            .unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                project_id,
                account,
                Role::Maintainer,
            )
            .unwrap();
            let plan = crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id,
                    title: "Release checklist".into(),
                    issue_id: None,
                    steps: vec![
                        crate::db::models::CreatePlanStep {
                            title: "Linked incomplete".into(),
                            description: "Still open".into(),
                            issue_id: Some(issue_id),
                            done: false,
                            steps: vec![],
                        },
                        crate::db::models::CreatePlanStep {
                            title: "Manual complete".into(),
                            description: "Ready".into(),
                            issue_id: None,
                            done: true,
                            steps: vec![],
                        },
                    ],
                },
            )
            .unwrap();
            (plan, account, issue_id)
        };
        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let linked = document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[0].id
                ))
                .unwrap(),
            )
            .next()
            .expect("linked step rendered");
        let detach = linked
            .select(&scraper::Selector::parse("button[aria-label='Detach issue']").unwrap())
            .next()
            .expect("maintainers can detach a linked issue from a plan step");
        assert_eq!(detach.text().collect::<String>(), "unlink");
        assert_eq!(detach.value().attr("title"), Some("Detach issue"));
        assert!(
            linked.text().collect::<String>().contains("ACC-1: done"),
            "linked issue label stays visible alongside the detach action",
        );
        let manual = document
            .select(
                &scraper::Selector::parse(&format!(
                    "article[data-plan-step='{}']",
                    plan.steps[1].id
                ))
                .unwrap(),
            )
            .next()
            .expect("unlinked step rendered");
        assert!(
            manual.text().collect::<String>().contains("Link an issue…"),
            "maintainers can link an issue to an unlinked step",
        );

        let handler = detach
            .value()
            .attr("data-topcoat-on:click")
            .expect("detach uses the native action handler");
        let emitted = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/anchor_handler.test.cjs",
            &serde_json::json!({
                "handler": handler,
                "signals": super::super::super::home_fixture::page_signals(&html),
                "response": serde_json::to_value("saved".to_owned().into_surrogate()).unwrap(),
            }),
        );
        assert_eq!(emitted["path"], "/__native_plans/mutate");
        let arguments = emitted["arguments"].clone();
        let expected_arguments = serde_json::to_value(
            (
                account,
                "ACC".to_owned(),
                plan.id,
                plan.steps[0].id,
                "unlink".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap();
        assert_eq!(arguments, expected_arguments);

        let before =
            crate::db::queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id).unwrap();
        assert_eq!(before.steps[0].issue_id, Some(issue_id));
        assert!(!before.steps[0].done);
        assert!(before.steps[1].done);
        assert_eq!(
            crate::db::queries::issue_status(&fixture.db.read().unwrap(), issue_id).unwrap(),
            "done",
        );

        let changed_account = serde_json::to_value(
            (
                account + 1,
                "ACC".to_owned(),
                plan.id,
                plan.steps[0].id,
                "unlink".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap();
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            changed_account,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

        {
            let conn = fixture.db.write().unwrap();
            let hidden_project_id =
                crate::db::queries::resolve_project_identifier(&conn, "HIDE").unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                hidden_project_id,
                account,
                Role::Maintainer,
            )
            .unwrap();
        }
        let wrong_plan_project = serde_json::to_value(
            (
                account,
                "HIDE".to_owned(),
                plan.id,
                plan.steps[0].id,
                "unlink".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap();
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            wrong_plan_project,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

        {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::members::upsert_member(&conn, project_id, account, Role::Viewer)
                .unwrap();
        }
        let (status, viewer_html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let viewer_document = scraper::Html::parse_document(&viewer_html);
        assert!(
            viewer_document
                .select(&scraper::Selector::parse("button[aria-label='Detach issue']").unwrap())
                .next()
                .is_none(),
            "viewers cannot detach linked issues",
        );
        assert!(
            !viewer_document
                .select(&scraper::Selector::parse("summary").unwrap())
                .any(|summary| summary.text().collect::<String>().contains("Link an issue")),
            "viewers cannot link issues",
        );
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

        {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                project_id,
                account,
                Role::Maintainer,
            )
            .unwrap();
        }
        let (status, outcome) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(
            outcome,
            serde_json::to_value("saved".to_owned().into_surrogate()).unwrap(),
        );
        let after =
            crate::db::queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id).unwrap();
        assert_eq!(after.steps[0].issue_id, None);
        assert_eq!(after.steps[0].done, before.steps[0].done);
        assert_eq!(
            after.steps[0].reopened_via_issue_at,
            before.steps[0].reopened_via_issue_at,
        );
        assert_eq!(after.steps[1].done, before.steps[1].done);
        assert_eq!(after.done_count, before.done_count);
        assert_eq!(after.step_count, before.step_count);
        assert_eq!(
            crate::db::queries::issue_status(&fixture.db.read().unwrap(), issue_id).unwrap(),
            "done",
        );
    }

    #[tokio::test]
    async fn native_plan_anchor_can_be_cleared_by_maintainer() {
        let fixture = super::super::super::home_fixture::fixture();
        let (plan, account, issue_id) = {
            let conn = fixture.db.write().unwrap();
            let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id;
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            let issue_id = crate::db::queries::resolve_identifier(&conn, "ACC-1").unwrap();
            let plan = crate::db::queries::plans::create_plan(
                &conn,
                &crate::db::models::CreatePlan {
                    project_id,
                    title: "Release checklist".into(),
                    issue_id: Some(issue_id),
                    steps: vec![crate::db::models::CreatePlanStep {
                        title: "Ship it".into(),
                        description: "Ready".into(),
                        issue_id: None,
                        done: false,
                        steps: vec![],
                    }],
                },
            )
            .unwrap();
            (plan, account, issue_id)
        };
        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let selector = scraper::Selector::parse("button[aria-label='Clear anchor']").unwrap();
        assert!(
            document
                .select(&scraper::Selector::parse("button[aria-label='Clear anchor']").unwrap())
                .next()
                .is_none(),
            "viewers cannot clear plan anchors",
        );

        {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                project_id,
                account,
                Role::Maintainer,
            )
            .unwrap();
        }
        let (status, html) = super::super::super::home_fixture::document(
            &fixture,
            "",
            &format!("/ACC/plans/{}", plan.id),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let (project_id, role, current_plan) = {
            let conn = fixture.db.read().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            let role = conn
                .query_row(
                    "SELECT role FROM project_members WHERE project_id = ?1 AND user_id = ?2",
                    rusqlite::params![project_id, account],
                    |row| row.get::<_, String>(0),
                )
                .unwrap();
            let plan = crate::db::queries::plans::get_plan(&conn, plan.id).unwrap();
            (project_id, role, plan)
        };
        let clear = document.select(&selector).next();
        let labels = document
            .select(&scraper::Selector::parse("button[aria-label]").unwrap())
            .filter_map(|button| button.value().attr("aria-label"))
            .collect::<Vec<_>>();
        assert!(
            document
                .select(&scraper::Selector::parse("textarea + div").unwrap())
                .next()
                .is_some(),
            "step description textarea closes before following controls",
        );
        assert!(
            clear.is_some(),
            "maintainer clear control missing: project_id={project_id}, role={role}, plan.issue_id={:?}, anchor_identifier={:?}, button labels={labels:?}",
            current_plan.issue_id,
            current_plan.anchor_identifier,
        );
        let clear = clear.unwrap();
        let handler = clear
            .value()
            .attr("data-topcoat-on:click")
            .expect("clearing the anchor runs the emitted native mutation handler");
        let saved_reply = serde_json::to_value("saved".to_owned().into_surrogate()).unwrap();
        let emitted = super::super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/plans/anchor_handler.test.cjs",
            &serde_json::json!({
                "handler": handler,
                "signals": super::super::super::home_fixture::page_signals(&html),
                "response": saved_reply.clone(),
            }),
        );
        let arguments = emitted["arguments"].clone();
        assert_eq!(emitted["path"], "/__native_plans/mutate");
        let expected_arguments = serde_json::to_value(
            (
                account,
                "ACC".to_owned(),
                plan.id,
                0_i64,
                "anchor".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap();
        assert_eq!(arguments, expected_arguments);
        assert_eq!(emitted["changed_signal_ids"].as_array().unwrap().len(), 1);
        assert_eq!(emitted["revision_before"], "0");
        assert_eq!(emitted["revision_after"], "1");

        let changed_account = serde_json::to_value(
            (
                account + 1,
                "ACC".to_owned(),
                plan.id,
                0_i64,
                "anchor".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap();
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            changed_account,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

        {
            let conn = fixture.db.write().unwrap();
            let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
            crate::db::queries::members::upsert_member(&conn, project_id, account, Role::Viewer)
                .unwrap();
        }
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(
            crate::db::queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id)
                .unwrap()
                .anchor_identifier
                .as_deref(),
            Some("ACC-1"),
            "a viewer cannot clear the anchor",
        );

        {
            let conn = fixture.db.write().unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap(),
                account,
                Role::Maintainer,
            )
            .unwrap();
        }
        let (status, outcome) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments.clone(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(outcome, saved_reply);
        let cleared =
            crate::db::queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id).unwrap();
        assert_eq!(cleared.issue_id, None);
        assert_eq!(cleared.anchor_identifier, None);
        assert_eq!(cleared.title, plan.title);
        assert_eq!(cleared.status, plan.status);
        assert_eq!(cleared.step_count, plan.step_count);
        assert_eq!(cleared.done_count, plan.done_count);
        assert_eq!(cleared.steps.len(), plan.steps.len());
        for (after, before) in cleared.steps.iter().zip(&plan.steps) {
            assert_eq!(after.id, before.id);
            assert_eq!(after.title, before.title);
            assert_eq!(after.description, before.description);
            assert_eq!(after.done, before.done);
        }

        {
            let conn = fixture.db.write().unwrap();
            crate::db::queries::plans::update_plan(
                &conn,
                plan.id,
                &UpdatePlan {
                    issue_id: Some(Some(issue_id)),
                    ..Default::default()
                },
            )
            .unwrap();
            crate::db::queries::members::upsert_member(
                &conn,
                crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap(),
                account,
                Role::Viewer,
            )
            .unwrap();
        }
        let (status, _) = super::super::super::home_fixture::procedure(
            &fixture,
            "/__native_plans/mutate",
            arguments,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(
            crate::db::queries::plans::get_plan(&fixture.db.read().unwrap(), plan.id)
                .unwrap()
                .anchor_identifier
                .as_deref(),
            Some("ACC-1"),
            "revoked maintainer access cannot clear the anchor",
        );
    }
}
