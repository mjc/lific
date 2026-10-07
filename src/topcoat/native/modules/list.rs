use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::mascot::Mascot;
use super::super::{context, icons, mascot, navigation, project_authority, session, transport};
use crate::{
    db::models::{CreateModule, Project},
    services::modules::{ModuleList, ModuleSummary},
};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

const STATUS_ORDER: [&str; 6] = [
    "active",
    "planned",
    "paused",
    "backlog",
    "done",
    "cancelled",
];
const STATUS_LABELS: [&str; 6] = [
    "Active",
    "Planned",
    "Paused",
    "Backlog",
    "Done",
    "Cancelled",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Active,
    Backlog,
    Archive,
    All,
}

impl Tab {
    fn parse(query: &str, summaries: &[ModuleSummary]) -> Self {
        let requested = serde_urlencoded::from_str::<Vec<(String, String)>>(query)
            .ok()
            .and_then(|pairs| {
                pairs
                    .into_iter()
                    .find(|(key, _)| key == "tab")
                    .map(|(_, value)| value)
            });
        match requested.as_deref() {
            Some("backlog") => Self::Backlog,
            Some("archive") => Self::Archive,
            Some("all") => Self::All,
            Some("active") => Self::Active,
            _ if !summaries.is_empty()
                && !summaries.iter().any(|item| {
                    matches!(item.module.status.as_str(), "active" | "planned" | "paused")
                }) =>
            {
                Self::All
            }
            _ => Self::Active,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Backlog => "backlog",
            Self::Archive => "archive",
            Self::All => "all",
        }
    }

    fn includes(self, status: &str) -> bool {
        match self {
            Self::Active => matches!(status, "active" | "planned" | "paused"),
            Self::Backlog => status == "backlog",
            Self::Archive => matches!(status, "done" | "cancelled"),
            Self::All => true,
        }
    }
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    account: i64,
    project: &Project,
    authority: &project_authority::Snapshot,
    data: ModuleList,
    query: &str,
) -> BoxView<'a> {
    let tab = Tab::parse(query, &data.modules);
    let selected = tab.as_str();
    let project_name = project.identifier.clone();
    let visible = data
        .modules
        .iter()
        .filter(|item| tab.includes(&item.module.status))
        .collect::<Vec<_>>();
    let groups = STATUS_ORDER
        .iter()
        .enumerate()
        .filter_map(|(index, status)| {
            let items = visible
                .iter()
                .copied()
                .filter(|item| item.module.status == *status)
                .collect::<Vec<_>>();
            (!items.is_empty()).then_some((
                status.to_string(),
                STATUS_LABELS[index].to_owned(),
                items,
            ))
        })
        .collect::<Vec<_>>();
    let known = STATUS_ORDER;
    let unknown = visible
        .iter()
        .copied()
        .filter(|item| !known.contains(&item.module.status.as_str()))
        .collect::<Vec<_>>();
    let owner = cx.keyed(format!("native-modules-list-{account}-{}", project.id));
    let creating = signal(&owner, || false);
    let draft = signal(&owner, String::new);
    let emoji = signal(&owner, String::new);
    let error = signal(&owner, String::new);
    let busy = signal(&owner, || false);
    let destination = transport::mounted_url(cx, &format!("/{project_name}/modules/"));
    let submit = create_attributes(
        cx,
        account,
        project.id,
        project_name.clone(),
        destination,
        creating.clone(),
        draft.clone(),
        emoji.clone(),
        error.clone(),
        busy.clone(),
    );
    let active_count = data
        .modules
        .iter()
        .filter(|m| Tab::Active.includes(&m.module.status))
        .count()
        .to_string();
    let backlog_count = data
        .modules
        .iter()
        .filter(|m| Tab::Backlog.includes(&m.module.status))
        .count()
        .to_string();
    let archive_count = data
        .modules
        .iter()
        .filter(|m| Tab::Archive.includes(&m.module.status))
        .count()
        .to_string();
    let all_count = data.modules.len().to_string();
    let active_class = tab_class(selected == "active");
    let backlog_class = tab_class(selected == "backlog");
    let archive_class = tab_class(selected == "archive");
    let all_class = tab_class(selected == "all");
    let empty_label = match tab {
        Tab::Active => "active",
        Tab::Backlog => "backlog",
        Tab::Archive => "archived",
        Tab::All => "",
    };
    let empty_heading = format!("No {empty_label} modules");
    let frac = if data.total_issues == 0 {
        0.0
    } else {
        data.done_issues as f64 / data.total_issues as f64
    };
    let active_url = navigation::attrs(cx, &format!("/{project_name}/modules?tab=active"));
    let backlog_url = navigation::attrs(cx, &format!("/{project_name}/modules?tab=backlog"));
    let archive_url = navigation::attrs(cx, &format!("/{project_name}/modules?tab=archive"));
    let all_url = navigation::attrs(cx, &format!("/{project_name}/modules?tab=all"));
    let active_modules = data.active_modules.to_string();
    let issue_total = data.total_issues.to_string();
    let done_total = data.done_issues.to_string();
    let progress = progress_ring(cx, frac, 88);
    let can_edit = authority.can_edit_structure;
    let plus_icon = icons::project_icon(cx, Some("Plus"), 14);
    let rows = groups
        .iter()
        .map(|(status, label, items)| status_group(cx, &project_name, status, label, items))
        .collect::<Vec<_>>();
    let unknown_rows = if unknown.is_empty() {
        None
    } else {
        Some(status_group(cx, &project_name, "other", "Other", &unknown))
    };
    let create_button = if can_edit {
        let creating = creating.clone();
        let error = error.clone();
        Some(view! {
            cx =>
            <button
                type="button"
                class="text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-2.5 py-1 rounded-md hover:opacity-90 inline-flex items-center gap-1"
                @click=$(|_event: Event| {
                    creating.set(true);
                    error.set("".to_owned());
                })
            >
                (plus_icon)
                "Module"
            </button>
        }.boxed())
    } else {
        None
    };
    let empty_state = if visible.is_empty() && !data.modules.is_empty() {
        Some(
            view! {
                cx =>
                <div class="flex flex-col items-center py-14 gap-3 text-center">
                    (mascot::render(cx, Mascot::Writing, 0.22))
                    <p class="text-heading font-medium text-[var(--text)]">
                        (empty_heading)
                    </p>
                    <p class="text-body-sm text-[var(--text-muted)]">
                        "Create a module to start organizing this slice of work."
                    </p>
                </div>
            }
            .boxed(),
        )
    } else {
        None
    };
    let no_modules = data.modules.is_empty();
    let aria_authority = authority.encoded();
    view! {
        owner =>
        <main
            data-native-modules=(project_name.clone())
            data-native-project-authority=(aria_authority)
            class="h-full overflow-y-auto"
        >
            <div class="max-w-[1100px] mx-auto px-6 py-6">
                <div class="flex items-center justify-between mb-5">
                    <h1 class="text-heading font-semibold text-[var(--text)] m-0">
                        "Modules"
                    </h1>
                    if let Some(button) = create_button {
                        (button)
                    }
                </div>
                <nav
                    aria-label="Module views"
                    class="mb-6 flex gap-5 border-b border-[var(--border)]"
                >
                    <a class=(active_class) (active_url)>
                        "Active "
                        (active_count)
                    </a>
                    <a class=(backlog_class) (backlog_url)>
                        "Backlog "
                        (backlog_count)
                    </a>
                    <a class=(archive_class) (archive_url)>
                        "Archive "
                        (archive_count)
                    </a>
                    <a class=(all_class) (all_url)>
                        "All "
                        (all_count)
                    </a>
                </nav>
                <section
                    class="mb-7 rounded-xl bg-[var(--surface)] p-5 shadow-[0_1px_2px_rgba(0,0,0,0.06)] flex items-center gap-6 flex-wrap"
                    aria-label="Module portfolio"
                >
                    (progress)
                    <div
                        class="grid grid-cols-2 sm:grid-cols-4 gap-x-8 gap-y-3 flex-1 min-w-[240px]"
                    >
                        (hero_stat(cx, data.modules.len().to_string(), "Modules"))
                        (hero_stat(cx, active_modules, "In flight"))
                        (hero_stat(cx, issue_total, "Issues"))
                        (hero_stat(cx, done_total, "Completed"))
                    </div>
                </section>
                if can_edit {
                    <form
                        class="mb-6 flex items-center gap-3 p-3 rounded-xl border-l-2 border-l-[var(--tc-btn-success)] bg-[var(--surface)]"
                        :hidden=$(if creating.get() { false } else { true })
                        (submit)
                    >
                        <input
                            class="w-10 bg-transparent text-center outline-none"
                            maxlength="12"
                            placeholder="Icon"
                            aria-label="Module icon"
                            :value=$(emoji.get())
                            @input=$(|event: Event| emoji.set(event.target.value))
                        />
                        <input
                            class="flex-1 bg-transparent outline-none text-body text-[var(--text)]"
                            placeholder="Module name (e.g. Q1 Launch, Auth, Search rework)"
                            :value=$(draft.get())
                            @input=$(|event: Event| draft.set(event.target.value))
                            @keydown=$(|event: Event| if event.key == "Escape" {
                                creating.set(false);
                            })
                        />
                        <p
                            role="alert"
                            class="text-body-sm text-[var(--error)]"
                            :hidden=$(error.get().is_empty())
                        >
                            $(error.get())
                        </p>
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
                                draft.set("".to_owned());
                                emoji.set("".to_owned());
                                error.set("".to_owned());
                            })
                        >
                            "Cancel"
                        </button>
                    </form>
                }
                if no_modules {
                    <div
                        class="flex flex-col items-center py-20 gap-4 px-6 max-w-[480px] mx-auto text-center"
                    >
                        (mascot::render(cx, Mascot::Writing, 0.25))
                        <p class="text-heading font-medium text-[var(--text)]">
                            "No moving parts yet"
                        </p>
                        <p
                            class="text-body-sm text-[var(--text-muted)] leading-relaxed"
                        >
                            "Modules gather related issues into a single arc of work: a feature, a release, an effort. Spin one up to start organizing."
                        </p>
                    </div>
                } else if let Some(empty) = empty_state {
                    (empty)
                } else {
                    for row in rows {
                        (row)
                    }
                    if let Some(row) = unknown_rows {
                        (row)
                    }
                }
            </div>
        </main>
    }.boxed()
}

fn status_group<'a>(
    cx: &'a Cx,
    project: &str,
    status: &str,
    label: &str,
    items: &[&ModuleSummary],
) -> BoxView<'a> {
    let cards = items
        .iter()
        .map(|item| module_card(cx, project, item))
        .collect::<Vec<_>>();
    let count = items.len().to_string();
    let label = label.to_owned();
    let class = if status == "active" {
        "grid grid-cols-1 sm:grid-cols-2 gap-3"
    } else {
        "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3"
    };
    view! {
        cx =>
        <section class="mb-8 last:mb-0">
            <div class="flex items-center gap-2 mb-3 px-1">
                <span
                    class="size-2.5 rounded-full bg-[var(--accent)]"
                    aria-hidden="true"
                ></span>
                <h2
                    class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]"
                >
                    (label)
                </h2>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">
                    (count)
                </span>
            </div>
            <div class=(class)>
                for card in cards {
                    (card)
                }
            </div>
        </section>
    }.boxed()
}

fn module_card<'a>(cx: &'a Cx, project: &str, item: &ModuleSummary) -> BoxView<'a> {
    let module = &item.module;
    let href = navigation::attrs(cx, &format!("/{project}/modules/{}", module.id));
    let fraction = if item.issue_count == 0 {
        0.0
    } else {
        item.done_count as f64 / item.issue_count as f64
    };
    let ring_size = if module.status == "active" { 56 } else { 46 };
    let progress = progress_ring(cx, fraction, ring_size);
    let icon = icons::project_icon(cx, module.emoji.as_deref(), 18);
    let title = module.name.clone();
    let tally = if item.issue_count == 0 {
        "No issues yet".to_owned()
    } else {
        format!("{}/{} done", item.done_count, item.issue_count)
    };
    let preview = description_preview(&module.description);
    view! {
        cx =>
        <a
            class="group flex items-start gap-3.5 rounded-xl bg-[var(--surface)] p-4 shadow-[0_1px_2px_rgba(0,0,0,0.06)] hover:shadow-[0_6px_16px_rgba(0,0,0,0.10)] transition motion-safe:hover:-translate-y-0.5 text-left no-underline"
            (href)
        >
            (progress)
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5">
                    (icon)
                    <span class="text-body-lg font-medium text-[var(--text)] truncate">
                        (title)
                    </span>
                </div>
                <p class="text-caption text-[var(--text-muted)] tabular-nums mt-1">
                    (tally)
                </p>
                if !preview.is_empty() {
                    <p
                        class="text-body-sm text-[var(--text-faint)] line-clamp-2 mt-1.5 leading-snug"
                    >
                        (preview)
                    </p>
                }
            </div>
        </a>
    }.boxed()
}

fn description_preview(description: &str) -> String {
    description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or_default()
        .replace(['*', '_', '`', '[', ']'], "")
        .trim()
        .chars()
        .take(120)
        .collect()
}

fn progress_ring<'a>(cx: &'a Cx, fraction: f64, size: usize) -> BoxView<'a> {
    let radius = 24.0;
    let circumference = std::f64::consts::TAU * radius;
    let dash_offset = circumference * (1.0 - fraction.clamp(0.0, 1.0));
    let size_class = if size >= 80 {
        "size-[88px]"
    } else if size >= 52 {
        "size-14"
    } else {
        "size-12"
    };
    let percent = format!("{}%", (fraction * 100.0).round() as i64);
    view! {
        cx =>
        <div
            class=(format!(
                "{size_class} relative shrink-0 flex items-center justify-center",
            ))
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
                    stroke-width="4"
                />
                <circle
                    cx="28"
                    cy="28"
                    r="24"
                    fill="none"
                    stroke="var(--success)"
                    stroke-width="4"
                    stroke-linecap="round"
                    stroke-dasharray=(format!("{circumference}"))
                    stroke-dashoffset=(format!("{dash_offset}"))
                />
            </svg>
            <span
                class="text-caption font-semibold tabular-nums text-[var(--text)] leading-none"
            >
                (percent)
            </span>
        </div>
    }
    .boxed()
}

fn hero_stat<'a>(cx: &'a Cx, value: String, label: &str) -> BoxView<'a> {
    let label = label.to_owned();
    view! {
        cx =>
        <div class="flex flex-col gap-1.5">
            <span class="text-heading font-display text-[var(--text)] tabular-nums">
                (value)
            </span>
            <span
                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
            >
                (label)
            </span>
        </div>
    }
    .boxed()
}

fn tab_class(selected: bool) -> &'static str {
    if selected {
        "border-b-2 border-[var(--accent)] pb-2 text-body-sm font-medium text-[var(--text)] no-underline"
    } else {
        "pb-2 text-body-sm text-[var(--text-muted)] no-underline"
    }
}

#[allow(clippy::too_many_arguments)]
fn create_attributes(
    cx: &Cx,
    account: i64,
    project_id: i64,
    project: String,
    destination: String,
    creating: Signal<bool>,
    draft: Signal<String>,
    emoji: Signal<String>,
    message: Signal<String>,
    busy: Signal<bool>,
) -> Attributes {
    let failure = message.clone();
    let failed_busy = busy.clone();
    let handler = expr!(|event: Event| {
        event.prevent_default();
        let name = draft.get().trim_ecmascript();
        if !busy.get() {
            if !name.is_empty() {
                busy.set(true);
                message.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failure.set("Couldn't create module. Try again.".to_owned());
                };
                let _run = async || {
                    let _id = create_module(account, project_id, project, name, emoji.get()).await;
                    busy.set(false);
                    creating.set(false);
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
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:submit",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

#[procedure("/__native_modules/create")]
async fn create_module(
    cx: &Cx,
    account: i64,
    project_id: i64,
    project: String,
    name: String,
    emoji: String,
) -> topcoat::Result<i64> {
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
    let expected_project = {
        let conn = db.read().map_err(topcoat::Error::from)?;
        crate::db::queries::resolve_project_identifier(&conn, &project)
            .map_err(topcoat::Error::from)?
    };
    if expected_project != project_id {
        return Err(crate::error::LificError::NotFound("project not found".into()).into());
    }
    session::read(
        cx,
        caller
            .scope(async move {
                crate::services::modules::create(
                    &db,
                    &hub,
                    &identity,
                    CreateModule {
                        project_id,
                        name: name.trim().to_owned(),
                        description: String::new(),
                        status: "active".to_owned(),
                        emoji: (!emoji.trim().is_empty()).then(|| emoji.trim().to_owned()),
                    },
                )
                .map(|module| module.id)
            })
            .await,
    )
}
