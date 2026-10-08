use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::{
    browser, context, dates, icons, markdown, mascot, navigation, project_authority, session,
    transport,
};
use crate::{
    db::models::{Module, Project, UpdateModule},
    services::modules::ModuleDetail,
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

const MODULE_STATUSES: [&str; 6] = [
    "active",
    "planned",
    "paused",
    "backlog",
    "done",
    "cancelled",
];
const MODULE_STATUS_LABELS: [&str; 6] = [
    "Active",
    "Planned",
    "Paused",
    "Backlog",
    "Done",
    "Cancelled",
];
const ISSUE_STATUS_ORDER: [&str; 5] = ["backlog", "todo", "active", "done", "cancelled"];

#[derive(Clone)]
struct ModuleMutation {
    account: i64,
    project_id: i64,
    module_id: i64,
    destination: String,
}

#[derive(Clone)]
struct StatusControls {
    value: Signal<String>,
    open: Signal<bool>,
    error: Signal<String>,
}

#[derive(Clone)]
struct DeleteMenuState {
    menu_open: Signal<bool>,
    confirming: Signal<bool>,
    deleting: Signal<bool>,
    error: Signal<String>,
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    account: i64,
    project: &Project,
    authority: &project_authority::Snapshot,
    data: ModuleDetail,
) -> BoxView<'a> {
    let owner = cx.keyed(format!("native-module-detail-{account}-{}", data.module.id));
    let module = data.module;
    let can_edit = authority.can_edit_structure;
    let can_create_issue = authority.can_edit_content;
    let project_identifier = project.identifier.clone();
    let route = transport::mounted_url(cx, &format!("/{project_identifier}/modules/{}", module.id));
    let title = signal(&owner, || module.name.clone());
    let title_draft = signal(&owner, || module.name.clone());
    let name_editing = signal(&owner, || false);
    let name_error = signal(&owner, String::new);
    let description = signal(&owner, || module.description.clone());
    let icon = signal(&owner, || module.emoji.clone().unwrap_or_default());
    let status = signal(&owner, || module.status.clone());
    let status_open = signal(&owner, || false);
    let status_error = signal(&owner, String::new);
    let status_controls = StatusControls {
        value: status,
        open: status_open,
        error: status_error,
    };
    let mutation = ModuleMutation {
        account,
        project_id: project.id,
        module_id: module.id,
        destination: route.clone(),
    };
    let props_open = signal(&owner, || false);
    let description_initial = module.description.clone();
    let issues = sorted_issues(data.issues);
    let progress_total = issues.len();
    let progress_done = issues
        .iter()
        .filter(|issue| issue.status.as_str() == "done")
        .count();
    let progress = if progress_total == 0 {
        0.0
    } else {
        progress_done as f64 / progress_total as f64
    };
    let progress_ring = if progress_total > 0 {
        Some(progress_view(cx, progress, progress_done, progress_total))
    } else {
        None
    };
    let status_counts = ISSUE_STATUS_ORDER.map(|status| {
        (
            status,
            issues
                .iter()
                .filter(|issue| issue.status.as_str() == status)
                .count(),
        )
    });
    let issue_rows = issues.iter().map(|issue| {
        let href = navigation::attrs(cx, &format!("/{project_identifier}/issues/{}", issue.identifier));
        let identifier = issue.identifier.clone();
        let issue_title = issue.title.clone();
        let status_icon = icons::status_icon(cx, issue.status, 14);
        let priority_icon = if issue.priority.as_str() == "none" { None } else { Some(icons::priority_icon(cx, issue.priority, 13)) };
        view! {
            cx =>
            <a
                class="flex items-center gap-3 px-2 py-2 rounded-md text-left hover:bg-[var(--bg-subtle)] transition-colors group no-underline"
                (href)
            >
                (status_icon)
                <span
                    class="text-caption font-mono text-[var(--text-faint)] shrink-0 tabular-nums w-[60px]"
                >
                    (identifier)
                </span>
                <span class="text-body text-[var(--text)] truncate flex-1">
                    (issue_title)
                </span>
                if let Some(priority) = priority_icon {
                    (priority)
                }
            </a>
        }.boxed()
    }).collect::<Vec<_>>();
    let create_issue = navigation::attrs(
        cx,
        &format!("/{project_identifier}/issues/new?module={}", module.id),
    );
    let create_first_issue = navigation::attrs(
        cx,
        &format!("/{project_identifier}/issues/new?module={}", module.id),
    );
    let name_trigger =
        name_trigger_attributes(cx, title.clone(), title_draft.clone(), name_editing.clone());
    let name_input = name_input_attributes(
        cx,
        mutation.clone(),
        title,
        title_draft.clone(),
        name_editing.clone(),
        name_error.clone(),
    );
    let save_description = update_attributes(
        cx,
        account,
        project.id,
        module.id,
        "description",
        description.clone(),
        route.clone(),
    );
    let save_icon = update_attributes(
        cx,
        account,
        project.id,
        module.id,
        "emoji",
        icon.clone(),
        route,
    );
    let status_trigger = status_trigger_attributes(cx, status_controls.open.clone());
    let status_choices = MODULE_STATUSES
        .into_iter()
        .zip(MODULE_STATUS_LABELS)
        .map(|(value, label)| {
            (
                value,
                label,
                status_choice_attributes(cx, mutation.clone(), value, status_controls.clone()),
            )
        })
        .collect::<Vec<_>>();
    let delete_menu = delete_menu(
        cx,
        account,
        project.id,
        module.id,
        issues.len(),
        module.name.clone(),
        project_identifier.clone(),
        DeleteMenuState {
            menu_open: signal(&owner, || false),
            confirming: signal(&owner, || false),
            deleting: signal(&owner, || false),
            error: signal(&owner, String::new),
        },
    );
    let shortcut = shortcut_attributes(cx, can_edit, props_open.clone());
    let authority_marker = authority.encoded();
    let name = module.name.clone();
    let description_html = if module.description.trim().is_empty() {
        None
    } else {
        Some(markdown::render(
            cx,
            &module.description,
            markdown::Scope::Private,
            &[],
        ))
    };
    let created_at = module.created_at.clone();
    let updated_at = module.updated_at.clone();
    let module_icon = icons::project_icon(cx, module.emoji.as_deref(), 20);
    let module_id = module.id;
    let issue_count = issues.len();
    let empty = issues.is_empty();
    let aside_status = status_sidebar(
        cx,
        &module,
        can_edit,
        status_controls,
        status_trigger,
        status_choices,
    );
    view! {
        owner =>
        <main
            data-native-module-detail=(module_id.to_string())
            data-native-project-authority=(authority_marker)
            class="h-full overflow-y-auto"
            (shortcut)
        >
            <div class="max-w-[1120px] mx-auto flex gap-0 min-h-full">
                <div class="flex-1 min-w-0 px-4 py-5 sm:px-8 sm:py-6">
                    <div class="flex items-center gap-3 mb-3">
                        if can_edit {
                            <form class="shrink-0 flex items-center gap-1" (save_icon)>
                                <input
                                    data-native-module-icon=""
                                    aria-label="Module icon"
                                    class="w-10 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-2 text-center"
                                    maxlength="12"
                                    :value=$(icon.get())
                                    @input=$(|event: Event| icon.set(event.target.value))
                                />
                                <button
                                    class="text-caption text-[var(--text-muted)]"
                                    type="submit"
                                >
                                    "Save icon"
                                </button>
                            </form>
                            <div class="flex-1 min-w-0">
                                <button
                                    type="button"
                                    class="text-left w-full rounded-md bg-transparent outline-none text-display font-display tracking-tight text-[var(--text)] py-1 hover:bg-[var(--bg-subtle)] cursor-text"
                                    aria-label=(format!("Edit module name: {name}"))
                                    (name_trigger)
                                    :hidden=$(name_editing.get())
                                >
                                    (name)
                                </button>
                                <input
                                    data-native-module-name-editor=""
                                    aria-label="Module name"
                                    class="w-full min-w-0 bg-transparent outline-none text-display font-display tracking-tight text-[var(--text)] py-1"
                                    :value=$(title_draft.get())
                                    :hidden=$(!name_editing.get())
                                    (name_input)
                                />
                                <p class="text-caption text-[var(--error)]" role="status">
                                    $(name_error.get())
                                </p>
                            </div>
                        } else {
                            <div
                                class="shrink-0 size-10 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] flex items-center justify-center"
                            >
                                (module_icon)
                            </div>
                            <h1
                                class="flex-1 min-w-0 text-display font-display tracking-tight text-[var(--text)] py-1"
                            >
                                (name)
                            </h1>
                        }
                        if let Some(ring) = progress_ring {
                            (ring)
                        }
                    </div>
                    <section class="mb-10">
                        if can_edit {
                            <form (save_description) class="flex flex-col gap-2">
                                <textarea
                                    data-native-module-description-editor=""
                                    class="w-full min-h-[120px] rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3 text-body text-[var(--text)]"
                                    placeholder="Describe this module... (markdown supported)"
                                    :value=$(description.get())
                                    @input=$(|event: Event| description.set(event.target.value))
                                ></textarea>
                                <div class="flex gap-2">
                                    <button
                                        type="submit"
                                        class="text-body-sm text-[var(--accent)]"
                                    >
                                        "Save description"
                                    </button>
                                    <button
                                        type="button"
                                        class="text-body-sm text-[var(--text-muted)]"
                                        @click=$(|_event: Event| description.set(
                                                description_initial.clone(),
                                            ))
                                    >
                                        "Cancel"
                                    </button>
                                </div>
                            </form>
                        } else if let Some(html) = description_html {
                            <article class="markdown-body prose max-w-none">
                                (topcoat::view::Unescaped::new_unchecked(html))
                            </article>
                        } else {
                            <p class="text-body-sm italic text-[var(--text-muted)]">
                                "No description"
                            </p>
                        }
                    </section>
                    <section>
                        <div class="flex items-baseline justify-between mb-3 pb-2">
                            <div class="flex items-baseline gap-2">
                                <h2
                                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]"
                                >
                                    "Issues"
                                </h2>
                                <span
                                    class="text-micro text-[var(--text-faint)] tabular-nums"
                                >
                                    (issue_count)
                                </span>
                            </div>
                            <div
                                class="flex items-center gap-3 text-micro text-[var(--text-faint)]"
                            >
                                for (issue_status, count) in status_counts {
                                    if count > 0 {
                                        <span class="flex items-center gap-1">
                                            <span class="tabular-nums">(count)</span>
                                            <span>(issue_status)</span>
                                        </span>
                                    }
                                }
                            </div>
                        </div>
                        if empty {
                            <div class="py-10 flex flex-col items-center gap-3">
                                (mascot::render(cx, mascot::Mascot::Writing, 0.18))
                                <p class="text-body text-[var(--text-muted)]">
                                    "Nothing assigned here yet"
                                </p>
                                if can_create_issue {
                                    <a
                                        class="text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-3 py-1.5 rounded-md no-underline"
                                        (create_first_issue)
                                    >
                                        "＋ Add the first issue"
                                    </a>
                                }
                            </div>
                        } else {
                            <div class="flex flex-col -mx-2">
                                for row in issue_rows {
                                    (row)
                                }
                            </div>
                        }
                    </section>
                </div>
                <button
                    class="md:hidden fixed bottom-4 right-4 z-30 rounded-full bg-[var(--surface)] shadow-lg p-3 text-[var(--text)]"
                    aria-label="Show details"
                    :aria-expanded=$(if props_open.get() { "true" } else { "false" })
                    @click=$(|_event: Event| props_open.set(!props_open.get()))
                >
                    "Details"
                </button>
                <aside
                    :class=$(if props_open.get() {
                        "w-[280px] sm:w-[300px] md:w-[236px] shrink-0 self-start overflow-y-auto bg-[var(--bg-subtle)] py-5 px-5 fixed inset-y-0 right-0 z-50 translate-x-0 shadow-2xl md:static md:translate-x-0 md:shadow-none md:rounded-xl md:my-6 md:mr-2"
                    } else {
                        "hidden md:block w-[236px] shrink-0 self-start bg-[var(--bg-subtle)] py-5 px-5 md:rounded-xl md:my-6 md:mr-2"
                    })
                >
                    <div class="md:hidden flex justify-end mb-2">
                        <button
                            class="text-body-sm text-[var(--text-muted)]"
                            aria-label="Close details"
                            @click=$(|_event: Event| props_open.set(false))
                        >
                            "Close"
                        </button>
                    </div>
                    (aside_status)
                    <div class="border-t border-[var(--border)] -mx-5 my-4"></div>
                    <div class="flex flex-col gap-4">
                        <div>
                            <p
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-0.5"
                            >
                                "Created"
                            </p>
                            <p
                                class="text-body-sm text-[var(--text-muted)] leading-snug m-0"
                            >
                                (dates::absolute_time_view(cx, &created_at))
                            </p>
                        </div>
                        <div>
                            <p
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-0.5"
                            >
                                "Updated"
                            </p>
                            <p
                                class="text-body-sm text-[var(--text-muted)] leading-snug m-0"
                            >
                                (dates::absolute_time_view(cx, &updated_at))
                            </p>
                        </div>
                    </div>
                    <div class="border-t border-[var(--border)] -mx-5 my-4"></div>
                    <div class="flex flex-col gap-2">
                        if can_create_issue {
                            <a
                                class="text-body-sm text-[var(--accent)] no-underline"
                                (create_issue)
                            >
                                "＋ Create issue in module"
                            </a>
                        }
                        if can_edit {
                            (delete_menu)
                        }
                    </div>
                </aside>
            </div>
        </main>
    }.boxed()
}

fn status_sidebar<'a>(
    cx: &'a Cx,
    module: &Module,
    can_edit: bool,
    controls: StatusControls,
    status_trigger: Attributes,
    status_choices: Vec<(&'static str, &'static str, Attributes)>,
) -> BoxView<'a> {
    let StatusControls {
        value: status,
        open: status_open,
        error: status_error,
    } = controls;
    let current = module.status.clone();
    let current_label = MODULE_STATUSES
        .iter()
        .position(|value| *value == current)
        .map_or_else(
            || current.clone(),
            |index| MODULE_STATUS_LABELS[index].to_owned(),
        );
    view! {
        cx =>
        <div class="mb-5">
            <p
                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2"
            >
                "Status"
            </p>
            if can_edit {
                <div class="relative">
                    <button
                        type="button"
                        class="rounded-md bg-[var(--surface)] px-2 py-1 text-body-sm text-[var(--text)]"
                        aria-label="Change module status"
                        :aria-expanded=$(if status_open.get() {
                            "true"
                        } else {
                            "false"
                        })
                        (status_trigger)
                    >
                        (current_label)
                    </button>
                    <div
                        class="absolute z-10 mt-1 flex min-w-full flex-col rounded-md border border-[var(--border)] bg-[var(--surface)] p-1 shadow-lg"
                        role="menu"
                        aria-label="Module statuses"
                        :hidden=$(!status_open.get())
                    >
                        for (value, label, attrs) in status_choices {
                            <button
                                class="block w-full rounded px-2 py-1 text-left text-body-sm text-[var(--text)] hover:bg-[var(--bg-subtle)]"
                                type="button"
                                role="menuitemradio"
                                :aria-checked=$(status.get() == value)
                                data-native-module-status-option=(value)
                                (attrs)
                            >
                                (label)
                            </button>
                        }
                    </div>
                    <p class="text-caption text-[var(--error)]" role="status">
                        $(status_error.get())
                    </p>
                </div>
            } else {
                <p class="text-body-sm text-[var(--text)] m-0">(current)</p>
            }
        </div>
    }.boxed()
}

fn sorted_issues(mut issues: Vec<crate::db::models::Issue>) -> Vec<crate::db::models::Issue> {
    issues.sort_by(|a, b| {
        let a_order = ISSUE_STATUS_ORDER
            .iter()
            .position(|status| *status == a.status.as_str())
            .unwrap_or(ISSUE_STATUS_ORDER.len());
        let b_order = ISSUE_STATUS_ORDER
            .iter()
            .position(|status| *status == b.status.as_str())
            .unwrap_or(ISSUE_STATUS_ORDER.len());
        a_order
            .cmp(&b_order)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    issues
}

fn progress_view<'a>(cx: &'a Cx, fraction: f64, done: usize, total: usize) -> BoxView<'a> {
    let radius = 24.0;
    let circumference = std::f64::consts::TAU * radius;
    let offset = circumference * (1.0 - fraction.clamp(0.0, 1.0));
    let percent = format!("{}%", (fraction * 100.0).round() as i64);
    let tally = format!("{done}/{total} done");
    view! {
        cx =>
        <div class="shrink-0 flex flex-col items-center gap-1 pl-2">
            <div
                class="size-14 relative flex items-center justify-center"
                role="img"
                aria-label=(format!("{percent} complete"))
            >
                <svg
                    class="absolute inset-0 size-full -rotate-90"
                    viewBox="0 0 56 56"
                    aria-hidden="true"
                >
                    <circle
                        cx="28"
                        cy="28"
                        r="24"
                        fill="none"
                        stroke="var(--bg-subtle)"
                        stroke-width="5"
                    />
                    <circle
                        cx="28"
                        cy="28"
                        r="24"
                        fill="none"
                        stroke="var(--success)"
                        stroke-width="5"
                        stroke-linecap="round"
                        stroke-dasharray=(format!("{circumference}"))
                        stroke-dashoffset=(format!("{offset}"))
                    />
                </svg>
                <span class="text-caption font-semibold tabular-nums">(percent)</span>
            </div>
            <span class="text-micro text-[var(--text-muted)] tabular-nums">
                (tally)
            </span>
        </div>
    }
    .boxed()
}

#[allow(clippy::too_many_arguments)]
fn update_attributes(
    cx: &Cx,
    account: i64,
    project_id: i64,
    module_id: i64,
    field: &'static str,
    value: Signal<String>,
    destination: String,
) -> Attributes {
    let field = field.to_owned();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let value = value.get();
        let _run = async || {
            update_module(account, project_id, module_id, field, value).await;
            raw!("cx.navigate(${destination}.toString());", ());
        };
        raw!("Promise.resolve().then(()=>${_run}());", ());
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn name_trigger_attributes(
    cx: &Cx,
    title: Signal<String>,
    draft: Signal<String>,
    editing: Signal<bool>,
) -> Attributes {
    let handler = expr!(|_event: Event| {
        draft.set(title.get());
        editing.set(true);
        raw!(
            "requestAnimationFrame(()=>${_event}.inner.currentTarget.parentElement?.querySelector('[data-native-module-name-editor]')?.focus())",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn name_input_attributes(
    cx: &Cx,
    mutation: ModuleMutation,
    title: Signal<String>,
    draft: Signal<String>,
    editing: Signal<bool>,
    error: Signal<String>,
) -> Attributes {
    let ModuleMutation {
        account,
        project_id,
        module_id,
        destination,
    } = mutation;
    let failed_error = error.clone();
    let input = expr!(|event: Event| {
        draft.set(event.target.value);
    });
    let finish = expr!(|event: Event| {
        let key = raw!("cx.hydrate(${event}.inner.key ?? '')", String::new());
        if key == "Escape" {
            event.prevent_default();
            draft.set(title.get());
            editing.set(false);
            return;
        }
        if key != "" {
            if key != "Enter" {
                return;
            }
        }
        if !editing.get() {
            return;
        }
        if key == "Enter" {
            event.prevent_default();
        }
        let before = title.get();
        let value = draft.get().trim_ecmascript().to_owned();
        editing.set(false);
        if value.is_empty() {
            draft.set(before);
            return;
        }
        if value == before {
            draft.set(before);
            return;
        }
        error.set("".to_owned());
        let _failed = || {
            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                failed_error.set("Unable to save module name.".to_owned());
            }
        };
        let _run = async || {
            update_module(account, project_id, module_id, "name".to_owned(), value).await;
            raw!(
                "if (!cx.abortSignal.aborted) cx.navigate(${destination}.toString());",
                ()
            );
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(3);
    attrs.insert(cx, "data-topcoat-on:input", input.into_evaluated_and_js().1);
    let finish_js = finish.into_evaluated_and_js().1;
    attrs.insert(cx, "data-topcoat-on:blur", finish_js.clone());
    attrs.insert(cx, "data-topcoat-on:keydown", finish_js);
    attrs
}

fn status_trigger_attributes(cx: &Cx, open: Signal<bool>) -> Attributes {
    let handler = expr!(|_event: Event| open.set(!open.get()));
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn status_choice_attributes(
    cx: &Cx,
    mutation: ModuleMutation,
    selected: &'static str,
    controls: StatusControls,
) -> Attributes {
    let ModuleMutation {
        account,
        project_id,
        module_id,
        destination,
    } = mutation;
    let StatusControls {
        value: status,
        open,
        error,
    } = controls;
    let failed_status = status.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let before = status.get();
        open.set(false);
        if before == selected {
            return;
        }
        status.set(selected.to_owned());
        error.set("".to_owned());
        let _failed = || {
            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                failed_status.set(before);
                failed_error.set("Unable to save module status.".to_owned());
            }
        };
        let _run = async || {
            update_module(
                account,
                project_id,
                module_id,
                "status".to_owned(),
                selected.to_owned(),
            )
            .await;
            raw!(
                "if (!cx.abortSignal.aborted) cx.navigate(${destination}.toString());",
                ()
            );
        };
        raw!(
            "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
            ()
        );
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn delete_menu<'a>(
    cx: &'a Cx,
    account: i64,
    project_id: i64,
    module_id: i64,
    issue_count: usize,
    name: String,
    project: String,
    state: DeleteMenuState,
) -> BoxView<'a> {
    let destination = transport::mounted_url(cx, &format!("/{project}/modules"));
    let confirm_body = module_delete_body(issue_count);
    let owner_id = format!("native-module-delete-{module_id}");
    let DeleteMenuState {
        menu_open,
        confirming,
        deleting,
        error,
    } = state;
    let browser = browser::bindings();
    let toggle_confirming = confirming.clone();
    let toggle_menu = menu_open.clone();
    let toggle = expr!(|event: Event| {
        event.stop_propagation();
        if browser.is_disposed() {
            return;
        }
        if !deleting.get() {
            if confirming.get() {
                toggle_confirming.set(false);
                toggle_menu.set(false);
            } else {
                menu_open.set(!menu_open.get());
            }
        }
    });
    let mut toggle_attrs = Attributes::with_capacity(1);
    toggle_attrs.insert(cx, "data-native-module-delete-action", "toggle");
    toggle_attrs.insert(
        cx,
        "data-topcoat-on:click",
        toggle.into_evaluated_and_js().1,
    );
    let open_confirming = confirming.clone();
    let open_menu = menu_open.clone();
    let open = expr!(|event: Event| {
        event.stop_propagation();
        if !browser.is_disposed() {
            if !deleting.get() {
                open_confirming.set(true);
                open_menu.set(true);
            }
        }
    });
    let mut open_attrs = Attributes::with_capacity(2);
    open_attrs.insert(cx, "data-native-module-delete-action", "open-confirm");
    open_attrs.insert(cx, "data-topcoat-on:click", open.into_evaluated_and_js().1);
    let cancel_confirming = confirming.clone();
    let cancel_menu = menu_open.clone();
    let cancel = expr!(|event: Event| {
        event.stop_propagation();
        if !browser.is_disposed() {
            if !deleting.get() {
                cancel_confirming.set(false);
                cancel_menu.set(false);
            }
        }
    });
    let mut cancel_attrs = Attributes::with_capacity(2);
    cancel_attrs.insert(cx, "data-native-module-delete-action", "cancel");
    cancel_attrs.insert(
        cx,
        "data-topcoat-on:click",
        cancel.into_evaluated_and_js().1,
    );

    let failed_deleting = deleting.clone();
    let failed_confirming = confirming.clone();
    let failed_menu = menu_open.clone();
    let failed_error = error.clone();
    let delete = expr!(async |event: Event| {
        event.stop_propagation();
        if browser.is_disposed() {
            return;
        }
        if !deleting.get() {
            deleting.set(true);
            error.set("".to_owned());
            let _failed = || {
                if !browser.is_disposed() {
                    failed_deleting.set(false);
                    failed_confirming.set(true);
                    failed_menu.set(true);
                    failed_error.set("Couldn't delete module. Try again.".to_owned());
                }
            };
            let _run = async || {
                if browser.is_disposed() {
                    return;
                }
                delete_module(account, project_id, module_id).await;
                if !browser.is_disposed() {
                    browser.navigate(destination.clone());
                }
            };
            browser.microtask(|| {
                if !browser.is_disposed() {
                    raw!(
                        "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                        ()
                    );
                }
            });
        }
    });
    let mut delete_attrs = Attributes::with_capacity(2);
    delete_attrs.insert(cx, "data-native-module-delete-action", "confirm");
    delete_attrs.insert(
        cx,
        "data-topcoat-on:click",
        delete.into_evaluated_and_js().1,
    );

    let mount_menu = menu_open.clone();
    let mount_confirming = confirming.clone();
    let dismiss_browser = browser::bindings();
    let dismiss_id = owner_id.clone();
    let mounted = expr!(|_mount: Event| {
        menu_open.set(false);
        confirming.set(false);
        deleting.set(false);
        let _dismiss = |event: Event| {
            if !dismiss_browser.is_disposed() {
                let outside = raw!(
                    "cx.hydrate(!document.getElementById(${dismiss_id}.toString())?.contains(${event}.inner.target))",
                    false
                );
                if outside {
                    mount_menu.set(false);
                    mount_confirming.set(false);
                }
            }
        };
        dismiss_browser.window_listener("click".to_owned(), _dismiss);
    });
    let mut mount_attrs = Attributes::with_capacity(2);
    mount_attrs.insert(cx, "data-native-module-delete", "");
    mount_attrs.insert(
        cx,
        "data-topcoat-on:mount",
        mounted.into_evaluated_and_js().1,
    );
    view! { cx =>
        <div id=(owner_id) class="relative" (mount_attrs)>
            <button type="button"
                class="grid size-7 place-items-center rounded-md text-[var(--text-faint)] hover:bg-[var(--bg-subtle)] hover:text-[var(--text)]"
                title="More actions" aria-label="More actions" data-native-module-delete-trigger="" (toggle_attrs)>
                (icons::ui_icon(cx, icons::UiIcon::MoreActions, 14))
            </button>
            <div data-native-module-delete-menu-panel="" class="absolute right-0 top-full z-30 mt-1.5 w-[180px] rounded-md border border-[var(--border)] bg-[var(--surface)] py-1 shadow-lg"
                :hidden=$(if menu_open.get() { confirming.get() } else { true })>
                <button type="button"
                    class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-body-sm text-[var(--error)] hover:bg-[var(--error-bg)]"
                    (open_attrs)>
                    (icons::ui_icon(cx, icons::UiIcon::Delete, 14)) "Delete module"
                </button>
            </div>
            <div data-native-module-delete-confirm-panel="" class="absolute right-0 top-full z-30 mt-1.5 w-[260px] rounded-md border border-[var(--border)] bg-[var(--surface)] p-3 shadow-lg"
                :hidden=$(!confirming.get())>
                <p class="mb-1 text-body-sm font-medium text-[var(--text)]">"Delete " (name) "?"</p>
                <p class="mb-3 text-caption text-[var(--text-muted)]">(confirm_body)</p>
                <div class="flex items-center gap-2">
                    <button type="button"
                        class="rounded-md bg-[var(--error)] px-3 py-1.5 text-body-sm font-medium text-[var(--error-text)] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled=$(deleting.get()) (delete_attrs)>
                        $(if deleting.get() { "Deleting..." } else { "Delete" })
                    </button>
                    <button type="button"
                        class="rounded-md px-3 py-1.5 text-body-sm text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
                        (cancel_attrs)>"Cancel"</button>
                </div>
            </div>
            <p data-native-module-delete-error="" class="mt-2 text-caption text-[var(--error)]" role="status"
                :hidden=$(error.get().is_empty())>$(error.get())</p>
        </div>
    }
    .boxed()
}

fn module_delete_body(issue_count: usize) -> String {
    if issue_count == 0 {
        "This module is empty. It will be removed.".to_owned()
    } else if issue_count == 1 {
        "1 issue will be unassigned from this module but not deleted.".to_owned()
    } else {
        format!("{issue_count} issues will be unassigned from this module but not deleted.")
    }
}

fn shortcut_attributes(cx: &Cx, can_edit: bool, props_open: Signal<bool>) -> Attributes {
    let handler = expr!(|event: Event| {
        if raw!("${event}.key === 'Escape'", false) {
            props_open.set(false);
        } else if can_edit {
            if raw!(
                "${event}.key.toLowerCase() !== 'e' || ${event}.ctrlKey || ${event}.metaKey || ${event}.altKey || ${event}.target.closest('input,textarea,select,[contenteditable=true],[role=dialog],[data-native-context-menu]')",
                true
            ) {
                return;
            }
            raw!(
                "document.querySelector('[data-native-module-description-editor]')?.focus()",
                ()
            );
            event.prevent_default();
        } else {
            return;
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:keydown",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

#[procedure("/__native_modules/update")]
async fn update_module(
    cx: &Cx,
    account: i64,
    project_id: i64,
    module_id: i64,
    field: String,
    value: String,
) -> topcoat::Result<()> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let db = context::db(cx).clone();
    let hub = app_context::<crate::realtime::RealtimeHub>(cx).clone();
    let identity = caller.identity.clone();
    session::read(
        cx,
        caller
            .scope(async move {
                let before = crate::services::modules::get(&db, &identity, module_id)?;
                if before.project_id != project_id {
                    return Err(crate::error::LificError::NotFound(
                        "module not found".into(),
                    ));
                }
                let input = match field.as_str() {
                    "name" => UpdateModule {
                        name: Some(value.trim().to_owned()),
                        ..Default::default()
                    },
                    "description" => UpdateModule {
                        description: Some(value),
                        ..Default::default()
                    },
                    "status" => UpdateModule {
                        status: Some(value),
                        ..Default::default()
                    },
                    "emoji" => UpdateModule {
                        emoji: Some((!value.trim().is_empty()).then(|| value.trim().to_owned())),
                        ..Default::default()
                    },
                    _ => {
                        return Err(crate::error::LificError::BadRequest(
                            "invalid module field".into(),
                        ));
                    }
                };
                crate::services::modules::update_scoped(
                    &db,
                    &hub,
                    &identity,
                    module_id,
                    Some(project_id),
                    input,
                )
                .map(|_| ())
            })
            .await,
    )?;
    Ok(())
}

#[procedure("/__native_modules/delete")]
async fn delete_module(
    cx: &Cx,
    account: i64,
    project_id: i64,
    module_id: i64,
) -> topcoat::Result<()> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let db = context::db(cx).clone();
    let hub = app_context::<crate::realtime::RealtimeHub>(cx).clone();
    let identity = caller.identity.clone();
    session::read(
        cx,
        caller
            .scope(async move {
                let module = crate::services::modules::get(&db, &identity, module_id)?;
                if module.project_id != project_id {
                    return Err(crate::error::LificError::NotFound(
                        "module not found".into(),
                    ));
                }
                crate::services::modules::delete_scoped(
                    &db,
                    &hub,
                    &identity,
                    module_id,
                    Some(project_id),
                )
            })
            .await,
    )
}

#[cfg(test)]
#[path = "detail_interactions.rs"]
mod interaction_tests;
