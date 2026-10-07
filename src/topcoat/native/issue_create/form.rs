use super::super::super::runtime::{
    signal_vec::{SignalVecExt, VecPositionExt},
    whitespace::StrEcmaTrimExt,
};
use super::super::transport;
use super::actions::{create as commit_create, create_label as commit_label};
use crate::db::models::{Label, Module, Priority};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

#[derive(Clone)]
pub(super) struct Form {
    account: i64,
    project_id: i64,
    project: String,
    title: Signal<String>,
    description: Signal<String>,
    status: Signal<String>,
    priority: Signal<String>,
    module_id: Signal<i64>,
    selected_labels: Signal<Vec<String>>,
    labels: Signal<Vec<(String, String)>>,
    label_name: Signal<String>,
    label_color: Signal<String>,
    busy: Signal<bool>,
    label_busy: Signal<bool>,
    error: Signal<String>,
    label_error: Signal<String>,
}

impl Form {
    pub(super) fn new(
        cx: &Cx,
        account: i64,
        project_id: i64,
        project: String,
        defaults: super::model::Defaults,
        labels: Vec<Label>,
    ) -> Self {
        Self {
            account,
            project_id,
            project,
            title: signal(cx, String::new),
            description: signal(cx, String::new),
            status: signal(cx, || defaults.status.as_str().to_owned()),
            priority: signal(cx, || Priority::None.as_str().to_owned()),
            module_id: signal(cx, || defaults.module_id.unwrap_or_default()),
            selected_labels: signal(cx, Vec::<String>::new),
            labels: signal(cx, || {
                labels
                    .into_iter()
                    .map(|label| (label.name, label.color))
                    .collect()
            }),
            label_name: signal(cx, String::new),
            label_color: signal(cx, || "#6366f1".to_owned()),
            busy: signal(cx, || false),
            label_busy: signal(cx, || false),
            error: signal(cx, String::new),
            label_error: signal(cx, String::new),
        }
    }
}

pub(super) fn views<'a>(
    cx: &'a Cx,
    form: &Form,
    modules: Vec<Module>,
    can_edit: bool,
    can_create_label: bool,
    authority: &str,
) -> (BoxView<'a>, BoxView<'a>) {
    let authority = authority.to_owned();
    let title = form.title.clone();
    let description = form.description.clone();
    let status = form.status.clone();
    let priority = form.priority.clone();
    let module_id = form.module_id.clone();
    let submit = submit(cx, form);

    let selected = form.selected_labels.clone();
    let labels = form.labels.clone();

    let module_options = modules
        .iter()
        .map(|module| {
            let id = module.id.to_string();
            let name = module.name.clone();
            view! { cx => <option value=(id)>(name)</option> }.boxed()
        })
        .collect::<Vec<_>>();

    let content = if can_edit {
        let label_create = if can_create_label {
            let create_label = create_label(cx, form);
            let label_name = form.label_name.clone();
            let label_color = form.label_color.clone();
            let label_error = form.label_error.clone();
            let label_busy = form.label_busy.clone();
            Some(view! {
                cx =>
                <div>
                    <div class="flex items-center gap-2">
                        <input
                            class="min-w-0 flex-1 text-body-sm bg-transparent border border-[var(--border)] rounded-md px-2 py-1.5"
                            aria-label="New label name"
                            placeholder="New label"
                            :value=$(label_name.get())
                            @input=$(|event: Event| label_name.set(event.target.value))
                        />
                        <input
                            class="size-7 p-0 border-0 bg-transparent"
                            type="color"
                            aria-label="Label color"
                            :value=$(label_color.get())
                            @input=$(|event: Event| label_color.set(event.target.value))
                        />
                        <button
                            class="text-body-sm text-[var(--accent)] disabled:opacity-40"
                            type="button"
                            :disabled=$(label_busy.get())
                            (create_label)
                        >
                            $(if label_busy.get() { "…" } else { "Add" })
                        </button>
                    </div>
                    <p
                        role="alert"
                        class="text-caption text-[var(--error)]"
                        :hidden=$(label_error.get().is_empty())
                    >
                        $(label_error.get())
                    </p>
                </div>
            }.boxed())
        } else {
            None
        };
        let status_values = [
            ("backlog", "Backlog"),
            ("todo", "Todo"),
            ("active", "Active"),
            ("done", "Done"),
            ("cancelled", "Cancelled"),
        ];
        let priority_values = [
            ("urgent", "Urgent"),
            ("high", "High"),
            ("medium", "Medium"),
            ("low", "Low"),
            ("none", "None"),
        ];
        let status_options = status_values
            .iter()
            .map(|(value, label)| {
                let value = (*value).to_owned();
                let label = (*label).to_owned();
                view! { cx => <option value=(value)>(label)</option> }.boxed()
            })
            .collect::<Vec<_>>();
        let priority_options = priority_values
            .iter()
            .map(|(value, label)| {
                let value = (*value).to_owned();
                let label = (*label).to_owned();
                view! { cx => <option value=(value)>(label)</option> }.boxed()
            })
            .collect::<Vec<_>>();

        view! {
            cx =>
            <section
                data-native-issue-create=""
                data-native-project-authority=(authority.to_owned())
                class="h-full min-h-0 overflow-y-auto"
            >
                <form
                    id="native-issue-create-form"
                    class="max-w-[960px] mx-auto flex flex-col md:flex-row min-h-full"
                    (submit)
                >
                    <div class="flex-1 min-w-0 px-4 sm:px-6 md:px-8 py-6">
                        <input
                            id="native-issue-create-title"
                            type="text"
                            autofocus="autofocus"
                            class="w-full text-title font-display tracking-tight bg-transparent border-none outline-none text-[var(--text)] py-1 mb-4 placeholder:text-[var(--text-faint)]"
                            placeholder="Issue title"
                            :value=$(title.get())
                            @input=$(|event: Event| title.set(event.target.value))
                        />
                        <section class="mb-8">
                            <textarea
                                id="native-issue-create-description"
                                class="w-full text-body leading-[1.7] text-[var(--text)] bg-transparent border-none outline-none resize-y p-0 m-0 font-[var(--font-body)] min-h-[120px]"
                                placeholder="Add a description... (markdown supported)"
                                :value=$(description.get())
                                @input=$(|event: Event| description.set(event.target.value))
                            ></textarea>
                            <div
                                class="flex items-center gap-2 mt-3 pt-3 border-t border-[var(--border)]"
                            >
                                <span class="text-caption text-[var(--text-faint)]">
                                    "Markdown supported"
                                </span>
                            </div>
                        </section>
                    </div>
                    <aside
                        class="w-full md:w-[220px] shrink-0 px-4 sm:px-6 md:px-5 py-6 md:border-l border-[var(--border)]"
                    >
                        <div class="flex flex-col gap-5">
                            <div class="flex flex-col gap-1.5">
                                <p
                                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Status"
                                </p>
                                <select
                                    class="w-full text-body-sm bg-transparent border border-[var(--border)] rounded-md px-2 py-1.5 text-[var(--text)]"
                                    :value=$(status.get())
                                    @change=$(|event: Event| status.set(event.target.value))
                                >
                                    for option in status_options {
                                        (option)
                                    }
                                </select>
                            </div>
                            <div class="flex flex-col gap-1.5">
                                <p
                                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Priority"
                                </p>
                                <select
                                    class="w-full text-body-sm bg-transparent border border-[var(--border)] rounded-md px-2 py-1.5 text-[var(--text)]"
                                    :value=$(priority.get())
                                    @change=$(|event: Event| priority.set(event.target.value))
                                >
                                    for option in priority_options {
                                        (option)
                                    }
                                </select>
                            </div>
                            if !modules.is_empty() {
                                <div class="flex flex-col gap-1.5">
                                    <p
                                        class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                    >
                                        "Module"
                                    </p>
                                    <select
                                        class="w-full text-body-sm bg-transparent border border-[var(--border)] rounded-md px-2 py-1.5 text-[var(--text)]"
                                        :value=$(module_id.get())
                                        @change=$(|_event: Event| {
                                            let parsed = raw!(
                                                "cx.hydrate({t:'i64',bits:64,v:BigInt(${_event}.target.value || 0).toString()})",
                                                0_i64,
                                            );
                                            module_id.set(parsed);
                                        })
                                    >
                                        <option value="0">"None"</option>
                                        for option in module_options {
                                            (option)
                                        }
                                    </select>
                                </div>
                            }
                            <div class="flex flex-col gap-2">
                                <p
                                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                                >
                                    "Labels"
                                </p>
                                issue_create_labels(labels: labels, selected: selected)
                                if let Some(label_create) = label_create {
                                    (label_create)
                                }
                            </div>
                        </div>
                    </aside>
                </form>
            </section>
        }
        .boxed()
    } else {
        let path = format!("/{}/issues", form.project);
        let back = super::super::transport::mounted_url(cx, &path);
        let back_attrs = super::super::navigation::attrs(cx, &path);
        let actions = view! {
            cx =>
            <a
                class="text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-3 py-1.5 rounded-md no-underline"
                href=(back)
                (back_attrs)
            >
                "Back to issues"
            </a>
        }.boxed();
        let error = super::super::error_state::surface(
            cx,
            "You can't create issues here",
            "You're a viewer on this project. Only maintainers and leads can create issues. You can still read and comment.",
            actions,
        );
        view! {
            cx =>
            <section
                data-native-issue-create=""
                data-native-project-authority=(authority.to_owned())
                class="h-full min-h-0"
            >
                (error)
            </section>
        }
        .boxed()
    };
    let topbar = topbar(cx, form, can_edit);
    (content, topbar)
}

#[shard("/__native_issue_create/labels")]
async fn issue_create_labels(
    cx: &Cx,
    labels: Signal<Vec<(String, String)>>,
    selected: Signal<Vec<String>>,
) -> topcoat::Result<impl View> {
    let selected_values = selected.get();
    let selected_text = selected_values.join(", ");
    let rows = labels
        .get()
        .into_iter()
        .map(|(name, color)| {
            let checked = selected_values.contains(&name);
            label_row(cx, selected.clone(), name, color, checked)
        })
        .collect::<Vec<_>>();
    Ok(view! {
        cx =>
        <div class="flex flex-col gap-1.5">
            for row in rows {
                (row)
            }
        </div>
        if !selected_text.is_empty() {
            <p class="text-caption text-[var(--text-muted)]">(selected_text)</p>
        }
    })
}

fn label_row<'a>(
    cx: &'a Cx,
    selected: Signal<Vec<String>>,
    name: String,
    color: String,
    checked: bool,
) -> BoxView<'a> {
    view! {
        cx =>
        <label
            class="flex items-center gap-2 text-body-sm text-[var(--text)] cursor-pointer"
        >
            <input
                type="checkbox"
                checked=(checked)
                @change=$(|event: Event| {
                    if event.target.checked {
                        selected.push(name.clone());
                    } else {
                        let index = selected.get().position(name.clone());
                        if index.is_some() {
                            selected.remove(index.unwrap());
                        }
                    }
                })
            />
            <span
                class="size-2 rounded-full"
                style=(format!("background-color:{color}"))
            ></span>
            (name)
        </label>
    }
    .boxed()
}

fn submit(cx: &Cx, form: &Form) -> Attributes {
    let account = form.account;
    let project_id = form.project_id;
    let title = form.title.clone();
    let description = form.description.clone();
    let status = form.status.clone();
    let priority = form.priority.clone();
    let module_id = form.module_id.clone();
    let labels = form.selected_labels.clone();
    let project = form.project.clone();
    let destination = transport::mounted_url(cx, &format!("/{project}/issues/"));
    let busy = form.busy.clone();
    let error = form.error.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let title_value = title.get().trim_ecmascript();
        if !busy.get() {
            if !title_value.is_empty() {
                busy.set(true);
                error.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_error.set(
                        "Couldn't create issue. Your draft is still here. Try again.".to_owned(),
                    );
                };
                let _run = async || {
                    let result = commit_create(
                        account,
                        project_id,
                        title_value,
                        description.get(),
                        status.get(),
                        priority.get(),
                        module_id.get(),
                        labels.get(),
                    )
                    .await;
                    busy.set(false);
                    if result.0 {
                        let _identifier = result.1;
                        raw!(
                            "cx.navigate(${destination}.toString()+${_identifier}.toString());",
                            ()
                        );
                    } else {
                        error.set(result.1);
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            } else {
                error.set("An issue title is required.".to_owned());
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn create_label(cx: &Cx, form: &Form) -> Attributes {
    let account = form.account;
    let project_id = form.project_id;
    let name = form.label_name.clone();
    let color = form.label_color.clone();
    let labels = form.labels.clone();
    let selected = form.selected_labels.clone();
    let busy = form.label_busy.clone();
    let error = form.label_error.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let value = name.get().trim_ecmascript();
        if !busy.get() {
            if !value.is_empty() {
                busy.set(true);
                error.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_error.set("Couldn't create label. Try again.".to_owned());
                };
                let _run = async || {
                    let result = commit_label(account, project_id, value, color.get()).await;
                    busy.set(false);
                    if result.0 {
                        let label_name = result.1;
                        let label_color = result.2;
                        labels.push((label_name.clone(), label_color));
                        selected.push(label_name);
                        name.set("".to_owned());
                    } else {
                        error.set(result.1);
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn topbar<'a>(cx: &'a Cx, form: &Form, can_edit: bool) -> BoxView<'a> {
    let error = form.error.clone();
    let busy = form.busy.clone();
    let title = form.title.clone();
    let back = transport::mounted_url(cx, &format!("/{}/issues", form.project));
    let create_label = if can_edit {
        "Create issue"
    } else {
        "New issue"
    };
    view! {
        cx =>
        <div class="flex items-center gap-3 px-6 py-2 w-full">
            <div class="flex items-center gap-1.5 shrink-0">
                <a
                    class="text-body-sm text-[var(--text-muted)] no-underline"
                    href=(back.clone())
                >
                    "← Issues"
                </a>
                <span class="text-[var(--text-faint)]">"/"</span>
                <span class="text-body-sm text-[var(--text-muted)]">"New issue"</span>
            </div>
            if can_edit {
                <div class="ml-auto flex items-center gap-2 shrink-0">
                    <span
                        role="alert"
                        class="text-body-sm text-[var(--error)]"
                        :hidden=$(error.get().is_empty())
                    >
                        $(error.get())
                    </span>
                    <a
                        class="text-body-sm text-[var(--text-muted)] px-2.5 py-1 rounded-md no-underline"
                        href=(back)
                    >
                        "Discard"
                    </a>
                    <button
                        class="text-body-sm font-medium text-[var(--accent-text)] bg-[var(--accent)] px-2.5 py-1 rounded-md disabled:opacity-40"
                        type="submit"
                        form="native-issue-create-form"
                        :disabled=$(if busy.get() {
                            true
                        } else {
                            title.get().trim_ecmascript().is_empty()
                        })
                    >
                        $(if busy.get() { "Creating…" } else { create_label })
                    </button>
                </div>
            }
        </div>
    }
    .boxed()
}
