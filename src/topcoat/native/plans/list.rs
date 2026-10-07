use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::{context, mascot, navigation, session, transport};
use crate::{
    db::models::{CreatePlan, Plan, Project, Role},
    error::LificError,
    realtime::RealtimeHub,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

const TABS: [&str; 4] = ["active", "done", "archived", "all"];

pub(super) fn content<'a>(
    cx: &'a Cx,
    project: &Project,
    account: i64,
    query: &str,
) -> topcoat::Result<BoxView<'a>> {
    let selected = route_tab(query);
    let owner = cx.keyed(format!("native-plan-list-{account}-{}", project.id));
    let caller = session::read(cx, context::caller(cx))?;
    let can_edit = match crate::authz::require_role(
        context::db(cx),
        &caller.identity,
        project.id,
        Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(LificError::Forbidden(_)) => false,
        Err(error) => return session::read(cx, Err(error)),
    };
    let active = signal(&owner, || selected);
    let creating = signal(&owner, || false);
    let draft = signal(&owner, || "".to_owned());
    let message = signal(&owner, || "".to_owned());
    let busy = signal(&owner, || false);
    let project_identifier = project.identifier.clone();
    let project_for_body = project_identifier.clone();
    let project_for_create = project_identifier.clone();
    let project_for_main = project_identifier.clone();
    let destination = transport::mounted_url(cx, &format!("/{project_identifier}/plans/"));
    let create = create_attributes(
        cx,
        account,
        project_for_create,
        destination,
        PlanCreateState {
            creating: creating.clone(),
            draft: draft.clone(),
            message: message.clone(),
            busy: busy.clone(),
        },
    );
    Ok(view! {
        owner =>
        <main data-native-plans=(project_for_main) class="h-full overflow-y-auto">
            <div class="max-w-[860px] mx-auto px-6 py-6">
                <div class="flex items-center justify-between mb-4">
                    <h1 class="text-heading font-semibold text-[var(--text)] m-0">
                        "Plans"
                    </h1>
                    if can_edit {
                        <button
                            type="button"
                            class="text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-2.5 py-1 rounded-md hover:opacity-90"
                            :hidden=$(if creating.get() {
                                true
                            } else {
                                if active.get() == "active" {
                                    false
                                } else {
                                    if active.get() == "all" { false } else { true }
                                }
                            })
                            @click=$(|_event: Event| {
                                draft.set("".to_owned());
                                message.set("".to_owned());
                                creating.set(true);
                            })
                        >
                            "＋ Plan"
                        </button>
                    }
                </div>
                if can_edit {
                    <form
                        class="mb-5 flex items-center gap-3 p-3 rounded-xl border-l-2 border-l-[var(--tc-btn-success)] bg-[var(--surface)]"
                        :hidden=$(if creating.get() {
                            if active.get() == "active" {
                                false
                            } else {
                                if active.get() == "all" { false } else { true }
                            }
                        } else {
                            true
                        })
                        (create)
                    >
                        <input
                            class="flex-1 bg-transparent outline-none text-body text-[var(--text)]"
                            placeholder="Plan title…"
                            :value=$(draft.get())
                            @input=$(|event: Event| draft.set(event.target.value))
                        />
                        <button
                            class="text-body-sm font-medium text-[var(--tc-btn-success)] hover:underline disabled:opacity-50"
                            type="submit"
                            :disabled=$(busy.get())
                        >
                            $(if busy.get() { "Creating…" } else { "Create" })
                        </button>
                        <button
                            type="button"
                            class="text-body-sm text-[var(--text-muted)]"
                            :disabled=$(busy.get())
                            @click=$(|_event: Event| {
                                creating.set(false);
                                message.set("".to_owned());
                            })
                        >
                            "Cancel"
                        </button>
                    </form>
                }
                <p
                    role="alert"
                    class="text-body-sm text-[var(--error)] mb-3"
                    :hidden=$(message.get().is_empty())
                >
                    $(message.get())
                </p>
                native_plan_list_body(
                    account: account,
                    project: project_for_body,
                    selected_tab: $(active.get()),
                    active: active,
                    creating_state: creating,
                    draft: draft
                )
            </div>
        </main>
    }.boxed())
}

struct PlanCreateState {
    creating: Signal<bool>,
    draft: Signal<String>,
    message: Signal<String>,
    busy: Signal<bool>,
}

fn create_attributes(
    cx: &Cx,
    account: i64,
    project: String,
    destination: String,
    state: PlanCreateState,
) -> Attributes {
    let PlanCreateState {
        creating,
        draft,
        message,
        busy,
    } = state;
    let failed_busy = busy.clone();
    let failed_message = message.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let title = draft.get().trim_ecmascript();
        if !busy.get() {
            if !title.is_empty() {
                busy.set(true);
                message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_message.set("Couldn't create plan. Try again.".to_owned());
                };
                let _run = async || {
                    let _id = create_plan(account, project, title).await;
                    busy.set(false);
                    creating.set(false);
                    draft.set("".to_owned());
                    raw!(
                        "cx.navigate(${destination}.toString()+${_id}.toString());",
                        ()
                    );
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
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

use row_shards::native_plan_list_body;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat generates flat shard handlers and drops function lint attributes"
)]
mod row_shards {
    use super::*;

    #[shard("/__native_plans/list")]
    pub(super) async fn native_plan_list_body(
        cx: &Cx,
        account: i64,
        project: String,
        selected_tab: String,
        active: Signal<String>,
        creating_state: Signal<bool>,
        draft: Signal<String>,
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
        let project_row = {
            let conn = context::db(cx).read()?;
            crate::db::queries::get_project(
                &conn,
                crate::db::queries::resolve_project_identifier(&conn, &project)?,
            )
        };
        let project_row = session::read(cx, project_row)?;
        let plans = session::read(
            cx,
            crate::services::plans::list_for_project(
                context::db(cx),
                &caller.identity,
                project_row.id,
            ),
        )?;
        let can_edit = match crate::authz::require_role(
            context::db(cx),
            &caller.identity,
            project_row.id,
            Role::Maintainer,
        ) {
            Ok(()) => true,
            Err(LificError::Forbidden(_)) => false,
            Err(error) => return session::read(cx, Err(error)),
        };
        let selected_rows = plans
            .iter()
            .filter(|plan| selected_tab == "all" || plan.status == selected_tab)
            .collect::<Vec<_>>();
        let selected_is_empty = selected_rows.is_empty();
        let show_empty_intro =
            plans.is_empty() && matches!(selected_tab.as_str(), "active" | "all");
        let mut tabs = Vec::new();
        for status in TABS {
            let count = plans
                .iter()
                .filter(|plan| status == "all" || plan.status == status)
                .count();
            tabs.push(tab_button(
                cx,
                status,
                count,
                active.clone(),
                creating_state.clone(),
            ));
        }
        let rows = if selected_tab == "all" {
            let mut groups = Vec::new();
            for status in ["active", "done", "archived"] {
                let group = plans
                    .iter()
                    .filter(|plan| plan.status == status)
                    .cloned()
                    .collect::<Vec<_>>();
                if !group.is_empty() {
                    let project_for_group = project.clone();
                    groups.push(view! {
                        cx =>
                        <section class="mb-6" data-plan-status=(status)>
                            <h2
                                class="text-micro font-semibold uppercase tracking-wide text-[var(--text-faint)] mb-2"
                            >
                                (format!("{} · {}", status_label(status), group.len()))
                            </h2>
                            <div class="flex flex-col gap-2">
                                for plan in group {
                                    (plan_card(cx, &project_for_group, plan))
                                }
                            </div>
                        </section>
                    }.boxed());
                }
            }
            groups
        } else if selected_is_empty {
            Vec::new()
        } else {
            let status = selected_tab.clone();
            let rows = selected_rows.into_iter().cloned().collect::<Vec<_>>();
            let project_for_rows = project;
            vec![
                view! { cx =>
                    <div class="flex flex-col gap-2" data-plan-status=(status)>
                        for plan in rows {(plan_card(cx, &project_for_rows, plan))}
                    </div>
                }
                .boxed(),
            ]
        };
        let empty_action = if can_edit && show_empty_intro {
            let creating = creating_state;
            Some(view! {
                cx =>
                <button
                    type="button"
                    class="mt-1 text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-3 py-1.5 rounded-md hover:opacity-90"
                    @click=$(move |_event: Event| {
                        draft.set("".to_owned());
                        creating.set(true);
                    })
                >
                    "＋ Create a plan"
                </button>
            }.boxed())
        } else {
            None
        };
        Ok(view! {
            cx =>
            <nav
                class="flex gap-1 p-1 rounded-lg bg-[var(--bg)] w-fit mb-6"
                aria-label="Plan status"
                role="tablist"
            >
                for tab in tabs {
                    (tab)
                }
            </nav>
            if show_empty_intro {
                <section
                    class="flex flex-col items-center py-16 gap-4 px-6 max-w-[480px] mx-auto text-center"
                >
                    (mascot::render(cx, mascot::Mascot::Writing, 0.25))
                    <div class="flex flex-col items-center gap-1.5">
                        <h1 class="text-heading font-medium text-[var(--text)]">
                            "The drawing board's empty"
                        </h1>
                        <p
                            class="text-body-sm text-[var(--text-muted)] leading-relaxed"
                        >
                            "A plan breaks a goal into a tree of steps that survives across sessions. Steps can mirror issues, so closing an issue checks off its step."
                        </p>
                    </div>
                    if let Some(button) = empty_action {
                        (button)
                    }
                </section>
            } else if selected_tab != "all" && selected_is_empty {
                <p class="py-16 text-center text-heading text-[var(--text-muted)]">
                    (empty_label(&selected_tab))
                </p>
            } else {
                for row in rows {
                    (row)
                }
            }
        })
    }
}

fn tab_button<'a>(
    cx: &'a Cx,
    status: &'static str,
    count: usize,
    active: Signal<String>,
    creating: Signal<bool>,
) -> BoxView<'a> {
    let label = format!("{} {count}", status_label(status));
    let id = status.to_owned();
    view! {
        cx =>
        <button
            type="button"
            role="tab"
            class="px-2.5 py-1 rounded-md border-0 bg-transparent text-body-sm text-[var(--text-muted)] hover:text-[var(--text)] aria-selected:bg-[var(--surface)] aria-selected:text-[var(--text)]"
            :aria-selected=$(active.get() == id)
            @click=$(move |_event: Event| {
                active.set(id.clone());
                if id == "done" {
                    creating.set(false);
                } else {
                    if id == "archived" {
                        creating.set(false);
                    }
                }
            })
        >
            (label)
        </button>
    }.boxed()
}

fn plan_card<'a>(cx: &'a Cx, project: &str, plan: Plan) -> BoxView<'a> {
    let fraction = if plan.step_count > 0 {
        plan.done_count as f64 / plan.step_count as f64
    } else {
        0.0
    };
    let circumference = std::f64::consts::TAU * 18.0;
    let dash_offset = circumference * (1.0 - fraction);
    let href = navigation::attrs(cx, &format!("/{project}/plans/{}", plan.id));
    let title = plan.title.clone();
    let identifier = format!(
        "{}{}",
        plan.identifier,
        plan.anchor_identifier
            .as_ref()
            .map_or_else(String::new, |anchor| format!(" · anchor {anchor}"))
    );
    let progress = format!("{}/{}", plan.done_count, plan.step_count);
    let has_steps = plan.step_count > 0;
    let percent = format!("{}", (fraction * 100.0).round() as i64);
    let label = format!("{percent}% complete");
    view! {
        cx =>
        <a
            class="group flex items-center gap-3.5 p-3 rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] hover:shadow-[0_6px_16px_rgba(0,0,0,0.10)] transition motion-safe:hover:-translate-y-0.5 text-left no-underline"
            (href)
        >
            <div
                class="size-10 shrink-0 relative flex items-center justify-center"
                role="img"
                aria-label=(label)
            >
                <svg
                    class="absolute inset-0 size-10 -rotate-90"
                    viewBox="0 0 40 40"
                    aria-hidden="true"
                >
                    <circle
                        cx="20"
                        cy="20"
                        r="18"
                        fill="none"
                        stroke="var(--bg-subtle)"
                        stroke-width="4"
                    />
                    <circle
                        cx="20"
                        cy="20"
                        r="18"
                        fill="none"
                        stroke="var(--success)"
                        stroke-width="4"
                        stroke-linecap="round"
                        stroke-dasharray=(format!("{circumference}"))
                        stroke-dashoffset=(format!("{dash_offset}"))
                    />
                </svg>
                if has_steps {
                    <span
                        class="text-micro font-semibold tabular-nums text-[var(--text)] leading-none"
                    >
                        (percent)
                    </span>
                } else {
                    <span class="text-[var(--text-faint)]" aria-hidden="true">
                        "☷"
                    </span>
                }
            </div>
            <div class="flex-1 min-w-0">
                <div class="text-body text-[var(--text)] truncate">(title)</div>
                <div class="text-caption text-[var(--text-faint)] font-mono">
                    (identifier)
                </div>
            </div>
            <div class="text-caption text-[var(--text-muted)] tabular-nums shrink-0">
                (progress)
            </div>
        </a>
    }.boxed()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "active" => "Active",
        "done" => "Done",
        "archived" => "Archived",
        "all" => "All",
        _ => "Other",
    }
}
fn empty_label(status: &str) -> &'static str {
    match status {
        "active" => "No active plans",
        "done" => "No completed plans",
        "archived" => "No archived plans",
        _ => "No plans",
    }
}
fn route_tab(query: &str) -> String {
    serde_urlencoded::from_str::<Vec<(String, String)>>(query)
        .ok()
        .and_then(|params| {
            params
                .into_iter()
                .find(|(key, _)| key == "status")
                .map(|(_, value)| value)
        })
        .filter(|tab| TABS.contains(&tab.as_str()))
        .unwrap_or_else(|| "active".to_owned())
}

#[procedure("/__native_plans/create")]
async fn create_plan(
    cx: &Cx,
    account: i64,
    project_identifier: String,
    title: String,
) -> topcoat::Result<i64> {
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
    let identity = caller.identity.clone();
    let db = context::db(cx).clone();
    let hub = app_context::<RealtimeHub>(cx).clone();
    let project_id = session::read(
        cx,
        (|| {
            let conn = db.read()?;
            crate::db::queries::resolve_project_identifier(&conn, &project_identifier)
        })(),
    )?;
    session::read(
        cx,
        caller
            .scope(async move {
                let input = CreatePlan {
                    project_id,
                    title: title.trim().to_owned(),
                    issue_id: None,
                    steps: Vec::new(),
                };
                crate::services::plans::create(&db, &hub, &identity, &input).map(|plan| plan.id)
            })
            .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plan_card_shows_svg_progress_and_anchor_identifier() {
        let cx = Cx::default();
        let plan = Plan {
            id: 9,
            project_id: 1,
            sequence: 9,
            identifier: "LIF-PLAN-9".into(),
            issue_id: None,
            anchor_identifier: Some("LIF-42".into()),
            title: "Release work".into(),
            status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
            steps: Vec::new(),
            step_count: 4,
            done_count: 1,
        };
        let html = plan_card(&cx, "LIF", plan)
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("stroke-dashoffset"));
        assert!(html.contains("25% complete"));
        assert!(html.contains("LIF-42"));
        assert!(html.contains("1/4"));
    }
}
