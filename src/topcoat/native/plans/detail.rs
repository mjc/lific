use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::{context, session, transport};
use crate::{
    db::models::{Plan, PlanStepNode, Project, Role, UpdatePlan},
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, shard, signal},
    view::{Attributes, BoxView, Unescaped, View, ViewExt, component, view},
};

#[derive(Clone)]
struct PlanEditor {
    revision: Signal<i64>,
    title_draft: Signal<String>,
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
    Ok(view! { cx =>
        native_plan_detail_body(account: account, project: project_identifier, plan_id: id)
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
    Ok(
        view! { owner => plan_detail_owner(account: account, project: project, plan: plan) }
            .boxed(),
    )
}

#[component]
async fn plan_detail_owner(
    cx: &Cx,
    account: i64,
    project: String,
    plan: Plan,
) -> topcoat::Result<impl View> {
    let revision = signal(cx, || 0_i64);
    let busy = signal(cx, || false);
    let message = signal(cx, String::new);
    let title_draft = signal(cx, || plan.title.clone());
    let step_title_target = signal(cx, || 0_i64);
    let step_title_draft = signal(cx, String::new);
    let step_description_target = signal(cx, || 0_i64);
    let step_description_draft = signal(cx, String::new);
    Ok(view! { cx =>
        <div data-native-plan-owner=(plan.identifier.clone())>
            <p class="text-body-sm text-[var(--error)] px-6 pt-3" role="alert" :hidden=$(message.get().is_empty())>$(message.get())</p>
            native_plan_saved(account: account, project: project, plan_id: plan.id, revision_value: $(revision.get()), revision_owner: revision, title_draft: title_draft, step_title_target: step_title_target, step_title_draft: step_title_draft, step_description_target: step_description_target, step_description_draft: step_description_draft, busy: busy, message: message)
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
        title_draft: Signal<String>,
        step_title_target: Signal<i64>,
        step_title_draft: Signal<String>,
        step_description_target: Signal<i64>,
        step_description_draft: Signal<String>,
        busy: Signal<bool>,
        message: Signal<String>,
    ) -> topcoat::Result<impl View> {
        let _ = revision_value;
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
        Ok(render_detail(
            cx,
            account,
            &project,
            plan,
            can_edit,
            PlanEditor {
                revision: revision_owner,
                title_draft,
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
    editor: PlanEditor,
) -> BoxView<'a> {
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
    let back = transport::mounted_url(cx, &format!("/{project}/plans"));
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
        Some(title_form(cx, account, project, plan.id, editor.clone()))
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
    view! { cx =>
        <main data-native-plan-detail=(identifier.clone()) class="h-full overflow-y-auto">
            <div class="max-w-[1080px] mx-auto px-6 py-6 grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_240px] gap-8">
                <section class="min-w-0">
                    <a class="text-caption text-[var(--text-muted)] hover:text-[var(--text)] no-underline" href=(back)>"‹ Plans"</a>
                    <div class="flex items-center gap-2 mt-3 mb-6"><span class="text-caption font-mono text-[var(--text-faint)]">(identifier)</span></div>
                    if let Some(editor) = title_editor { (editor) } else { <h1 class="text-heading font-medium text-[var(--text)]">(title)</h1> }
                    <div class="flex flex-col gap-0.5 mt-5">for step in steps {(step)}</div>
                    if let Some(add) = add_root { (add) }
                </section>
                <aside class="text-body-sm text-[var(--text-muted)]">
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Status"</p>
                        <div class="relative flex flex-col gap-1">for status in statuses {if let Some(status) = status {(status)}}</div>
                    </div>
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Progress"</p>
                        <div class="flex items-center gap-2"><div class="flex-1 h-1.5 rounded-full bg-[var(--bg-subtle)] overflow-hidden"><div class="h-full bg-[var(--accent)] rounded-full transition-all" style=(progress_width)></div></div><span class="text-caption text-[var(--text-muted)] tabular-nums">(progress_text)</span></div>
                    </div>
                    <div class="issue-meta-field py-3 border-b border-[var(--border)]">
                        <p class="issue-meta-field-label">"Anchor issue"</p>
                        if let Some(anchor) = anchor.as_deref() { if let Some(href) = anchor_href { <a class="font-mono text-[var(--accent)] hover:underline inline-flex items-center gap-1" href=(href)><span>(anchor.to_owned())</span><span aria-hidden="true">"↗"</span></a> } } else { <span>"None"</span> }
                        if let Some(editor) = anchor_editor { (editor) }
                    </div>
                    <div class="issue-meta-dates py-3"><div class="issue-meta-field mb-3"><p class="issue-meta-field-label">"Created"</p><p class="m-0">(super::super::dates::absolute_time_view(cx, &dates.0))</p></div><div class="issue-meta-field"><p class="issue-meta-field-label">"Updated"</p><p class="m-0">(super::super::dates::absolute_time_view(cx, &dates.1))</p></div></div>
                    if let Some(button) = delete { <div class="border-t border-[var(--border)] pt-3">(button)</div> }
                </aside>
            </div>
        </main>
    }.boxed()
}

#[allow(clippy::too_many_arguments)]
fn step_node<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    step: PlanStepNode,
    can_edit: bool,
    depth: usize,
    editor: PlanEditor,
) -> BoxView<'a> {
    let row_cx = cx.keyed(step.id);
    let project = project.to_owned();
    view! { row_cx => plan_step_component(account: account, project: project, plan_id: plan_id, step: step, can_edit: can_edit, depth: depth, revision: editor.revision, title_draft: editor.title_draft, step_title_target: editor.step_title_target, step_title_draft: editor.step_title_draft, step_description_target: editor.step_description_target, step_description_draft: editor.step_description_draft, busy: editor.busy, message: editor.message) }.boxed()
}

#[component]
async fn plan_step_component(
    cx: &Cx,
    account: i64,
    project: String,
    plan_id: i64,
    step: PlanStepNode,
    can_edit: bool,
    depth: usize,
    revision: Signal<i64>,
    title_draft: Signal<String>,
    step_title_target: Signal<i64>,
    step_title_draft: Signal<String>,
    step_description_target: Signal<i64>,
    step_description_draft: Signal<String>,
    busy: Signal<bool>,
    message: Signal<String>,
) -> topcoat::Result<impl View> {
    let editor = PlanEditor {
        revision,
        title_draft,
        step_title_target,
        step_title_draft,
        step_description_target,
        step_description_draft,
        busy,
        message,
    };
    let step_id = step.id;
    let title = step.title.clone();
    let description = if step.description.trim().is_empty() {
        None
    } else {
        Some(super::super::markdown::render(
            cx,
            &step.description,
            super::super::markdown::Scope::Private,
            &[],
        ))
    };
    let issue_identifier = step.issue_identifier.clone();
    let issue_status = step.issue_status.clone().unwrap_or_else(|| "?".to_owned());
    let issue_href = issue_identifier
        .as_ref()
        .map(|identifier| issue_href(cx, &project, identifier));
    let done = step.done;
    let children = step
        .children
        .into_iter()
        .map(|child| {
            step_node(
                cx,
                account,
                &project,
                plan_id,
                child,
                can_edit,
                depth + 1,
                editor.clone(),
            )
        })
        .collect::<Vec<_>>();
    let title_editor = if can_edit {
        Some(step_title_form(
            cx,
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
    let description_editor = if can_edit {
        Some(description_form(
            cx,
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
            cx,
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
    let remove = if can_edit {
        Some(action_button(
            cx,
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
            cx,
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
    Ok(OkBox::render(
        cx,
        step_id,
        title,
        description,
        issue_identifier,
        issue_status,
        issue_href,
        done,
        children,
        title_editor,
        description_editor,
        link_editor,
        remove,
        toggle,
        padding,
    ))
}

fn issue_href(cx: &Cx, fallback_project: &str, identifier: &str) -> String {
    let project = identifier
        .rsplit_once('-')
        .map_or(fallback_project, |(prefix, _)| prefix);
    transport::mounted_url(cx, &format!("/{project}/issues/{identifier}"))
}

struct OkBox;

impl OkBox {
    #[allow(clippy::too_many_arguments)]
    fn render<'a>(
        cx: &'a Cx,
        step_id: i64,
        title: String,
        description: Option<String>,
        issue_identifier: Option<String>,
        issue_status: String,
        issue_href: Option<String>,
        done: bool,
        children: Vec<BoxView<'a>>,
        title_editor: Option<BoxView<'a>>,
        description_editor: Option<BoxView<'a>>,
        link_editor: Option<BoxView<'a>>,
        remove: Option<BoxView<'a>>,
        toggle: Option<BoxView<'a>>,
        padding: usize,
    ) -> BoxView<'a> {
        let title_class = if done {
            "text-left text-body text-[var(--text-faint)] line-through"
        } else {
            "text-left text-body text-[var(--text)]"
        };
        view! { cx =>
                <article id=(format!("native-plan-step-{step_id}")) class="group flex flex-col py-1 rounded-md hover:bg-[var(--bg-subtle)]" data-plan-step=(step_id.to_string())>
                <div class="flex items-start gap-2" style=(format!("padding-left: min({}rem, 25%)", padding as f64 * 1.5))>
                    if let Some(toggle) = toggle { (toggle) } else { <span class="mt-0.5 size-4 shrink-0 rounded border flex items-center justify-center" aria-label=(if done { "Done" } else { "Not done" })>{if done { "✓" } else { "○" }}</span> }
                    <div class="flex-1 min-w-0">
                        if let Some(editor) = title_editor { <div><span class=(title_class)>(title.clone())</span>(editor)</div> } else { <span class=(title_class)>(title.clone())</span> }
                        if let Some(identifier) = issue_identifier { if let Some(href) = issue_href { <a class="inline-flex items-center gap-1 mt-1 text-micro font-mono text-[var(--accent)] hover:underline no-underline" href=(href)>(format!("{}: {}", identifier, issue_status))<span aria-hidden="true">"↗"</span></a> } }
                        if let Some(body) = description { <div class="prose-step mt-1 text-body-sm">(Unescaped::new_unchecked(body))</div> }
                        if let Some(editor) = description_editor { (editor) }
                        if let Some(editor) = link_editor { (editor) }
                        if let Some(remove) = remove { (remove) }
                    </div>
                </div>
                for child in children {(child)}
            </article>
        }.boxed()
    }
}

fn title_form<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    plan_id: i64,
    editor: PlanEditor,
) -> BoxView<'a> {
    let draft = editor.title_draft;
    let revision = editor.revision;
    let busy = editor.busy;
    let message = editor.message;
    let project = project.to_owned();
    let submit_draft = draft.clone();
    let submit_revision = revision;
    let submit_busy = busy.clone();
    let submit_message = message.clone();
    let failed_busy = busy.clone();
    let failed_message = message;
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let title = submit_draft.get().trim_ecmascript();
        if !submit_busy.get() {
            if !title.is_empty() {
                submit_busy.set(true);
                submit_message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Unable to save plan title.".to_owned());
                };
                let _run = async || {
                    mutate_plan(account, project, plan_id, 0_i64, "title".to_owned(), title).await;
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
    view! { cx => <form class="flex items-center gap-2 my-2" (submit)><input class="flex-1 bg-transparent outline-none text-heading text-[var(--text)] border-b border-[var(--accent)]" :value=$(draft.get()) @input=$(move |event: Event| draft.set(event.target.value))/><button type="submit" class="text-caption text-[var(--accent)] hover:underline" :disabled=$(busy.get())>"Save title"</button></form> }.boxed()
}

fn description_form<'a>(
    cx: &'a Cx,
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
    view! { cx => <div class="mt-1"><button type="button" class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]" (edit_attrs)>"Edit details"</button><form class="mt-2" (submit) :hidden=$(target.get()!=step_id)><textarea class="w-full bg-transparent outline-none text-body-sm leading-relaxed text-[var(--text)] border border-[var(--border)] rounded-md p-2 resize-y min-h-[80px]" :value=$(draft.get()) @input=$(move |event: Event| draft.set(event.target.value)) placeholder="Describe this step… (markdown supported)"/><div class="flex items-center gap-2 mt-1"><button class="text-caption font-medium text-[var(--accent-text)] bg-[var(--accent)] px-2 py-1 rounded-md" type="submit" :disabled=$(busy.get())>"Save"</button></div></form></div> }.boxed()
}

fn step_title_form<'a>(
    cx: &'a Cx,
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
    view! { cx => <div class="my-1"><button type="button" class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]" (edit_attrs)>"Edit title"</button><form class="flex items-center gap-2 my-2" (submit) :hidden=$(target.get()!=step_id)><input class="flex-1 bg-transparent outline-none text-body text-[var(--text)] border-b border-[var(--accent)]" :value=$(draft.get()) @input=$(move |event: Event| draft.set(event.target.value))/><button type="submit" class="text-caption text-[var(--accent)] hover:underline" :disabled=$(busy.get())>"Save"</button></form></div> }.boxed()
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
    view! { cx => <form class="mt-3 flex items-center gap-2" (submit)><input class="flex-1 bg-transparent outline-none text-body text-[var(--text)] border-b border-[var(--border)]" placeholder=(placeholder) :value=$(draft.get()) @input=$(move |event: Event| draft.set(event.target.value))/><button class="text-body-sm text-[var(--accent)] hover:underline" type="submit" :disabled=$(busy.get())>"Add"</button></form> }.boxed()
}

fn link_form<'a>(
    cx: &'a Cx,
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
    view! { cx => <details class="mt-2"><summary class="text-caption text-[var(--text-faint)] hover:text-[var(--text)]">(summary)</summary><form class="mt-2 flex items-center gap-2" (submit)><input class="w-32 bg-transparent outline-none font-mono text-caption text-[var(--text)] border-b border-[var(--border)]" placeholder="LIF-42" :value=$(draft.get()) @input=$(move |event: Event| draft.set(event.target.value))/><button class="text-caption text-[var(--accent)] hover:underline" type="submit" :disabled=$(busy.get())>"Link"</button></form></details> }.boxed()
}

fn action_button<'a>(
    cx: &'a Cx,
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
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let handler = expr!(|_event: Event| {
        let confirmed = if delete_action {
            raw!(
                "cx.hydrate(window.confirm('Delete this plan? This cannot be undone.'))",
                false
            )
        } else {
            true
        };
        if confirmed {
            if !busy.get() {
                busy.set(true);
                message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Unable to update plan.".to_owned());
                };
                let _run = async || {
                    mutate_plan(account, project, plan_id, target_id, request_action, value).await;
                    busy.set(false);
                    if delete_action {
                        raw!("window.location.assign(${destination}.toString());", ());
                    } else {
                        revision.set(revision.get() + 1_i64);
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! { cx => <button type="button" aria-label=(aria) class=(format!("w-full text-left px-2 py-1 rounded-md text-body-sm capitalize {class}")) (attributes)>(label)</button> }.boxed()
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
                    .map(|_| "saved".to_owned()),
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
                title_draft: signal(cx, String::new),
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
}
