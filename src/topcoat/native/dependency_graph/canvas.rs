//! Rust-rendered relation canvas and its native graph controls.
use std::collections::BTreeMap;

use super::actions::{
    link as link_relation, reverse as reverse_relation, unlink as unlink_relation,
};
use super::model::{self, Point, RelationEdge, RelationKind};
use crate::{
    db::models::{Issue, ProjectRelation},
    services::dependency_graph::DependencyGraphData,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, StringSurrogate, signal},
    view::{BoxView, View, ViewExt, component, view},
};

pub(super) fn content<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    data: DependencyGraphData,
    authority: crate::services::project_authority::Snapshot,
) -> BoxView<'a> {
    let project_id = data.project.id;
    let issues = data.issues;
    let relations = data.relations;
    let canvas = signal(cx, || "linked".to_owned());
    let closed = signal(cx, || false);
    let issue_revision = signal(cx, || 0_usize);
    let busy = signal(cx, || false);
    let error = signal(cx, String::new);
    let menu_kind = signal(cx, String::new);
    let menu_source = signal(cx, String::new);
    let menu_target = signal(cx, String::new);
    let project = project.to_owned();
    view! {
        cx =>
        graph_content(
            account: account,
            project: project,
            project_id: project_id,
            issues: issues,
            relations: relations,
            authority: authority,
            canvas: canvas,
            closed: closed,
            revision: issue_revision,
            busy: busy,
            error: error,
            menu_kind: menu_kind,
            menu_source: menu_source,
            menu_target: menu_target
        )
    }
    .boxed()
}

#[component]
async fn graph_content(
    cx: &Cx,
    account: i64,
    project: String,
    project_id: i64,
    issues: Vec<Issue>,
    relations: Vec<ProjectRelation>,
    authority: crate::services::project_authority::Snapshot,
    canvas: Signal<String>,
    closed: Signal<bool>,
    revision: Signal<usize>,
    busy: Signal<bool>,
    error: Signal<String>,
    menu_kind: Signal<String>,
    menu_source: Signal<String>,
    menu_target: Signal<String>,
) -> topcoat::Result<impl View> {
    let editable = authority.can_edit_content;
    let linked_selected = canvas.clone();
    let linked_activate = canvas.clone();
    let unlinked_selected = canvas.clone();
    let unlinked_activate = canvas.clone();
    let closed_toggle = closed.clone();
    let closed_text = closed.clone();
    let close_filter = closed.clone();
    let linked_label = format!("Linked {}", linked_count(&issues, &relations, closed.get()));
    let unlinked_label = format!(
        "Unlinked {}",
        unlinked_count(&issues, &relations, closed.get())
    );
    let linked_view = view! {
        cx =>
        <button type="button" data-native-graph-view="linked" aria-label="Show linked issues" :aria-pressed=$(if linked_selected.get() == "linked" { "true" } else { "false" }) class="rounded-md px-3 py-1.5 text-body-sm font-medium border border-[var(--border)]" @click=$(|_event: Event| linked_activate.set("linked".to_owned()))>(linked_label)</button>
    }.boxed();
    let unlinked_view = view! {
        cx =>
        <button type="button" data-native-graph-view="unlinked" aria-label="Show unlinked issues" :aria-pressed=$(if unlinked_selected.get() == "unlinked" { "true" } else { "false" }) class="rounded-md px-3 py-1.5 text-body-sm font-medium border border-[var(--border)]" @click=$(|_event: Event| unlinked_activate.set("unlinked".to_owned()))>(unlinked_label)</button>
    }.boxed();
    let close_button = view! {
        cx =>
        <button type="button" data-native-graph-closed="" :aria-pressed=$(if closed_toggle.get() { "true" } else { "false" }) class="inline-flex items-center gap-2 text-body-sm text-[var(--text-muted)]" @click=$(|_event: Event| close_filter.set(!close_filter.get()))>
            <span class="inline-flex size-4 items-center justify-center rounded border border-[var(--border)]">$(if closed_text.get() { "✓" } else { "" })</span>
            "Show closed"
        </button>
    }.boxed();
    let projection = model::project(&issues, &relations, closed.get());
    let (chosen, layout) = if canvas.get() == "unlinked" {
        let layout = model::layout_unlinked(
            &projection
                .unlinked
                .issues
                .iter()
                .map(|issue| issue.id)
                .collect::<Vec<_>>(),
        );
        (projection.unlinked, layout)
    } else {
        let blocking = edges(&projection.linked.relations)
            .into_iter()
            .filter(|edge| edge.kind == RelationKind::Blocks)
            .collect::<Vec<_>>();
        let cluster = edges(&projection.linked.relations);
        let layout = model::layout_linked(
            &projection
                .linked
                .issues
                .iter()
                .map(|issue| issue.id)
                .collect::<Vec<_>>(),
            &blocking,
            &cluster,
        );
        (projection.linked, layout)
    };
    let issue_index: BTreeMap<_, _> = chosen
        .issues
        .iter()
        .map(|issue| (issue.id, issue))
        .collect();
    let nodes = chosen
        .issues
        .iter()
        .map(|issue| {
            let point = layout
                .positions
                .get(&issue.id)
                .copied()
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            node(
                cx,
                issue,
                point,
                editable,
                &menu_kind,
                &menu_source,
                &menu_target,
            )
        })
        .collect::<Vec<_>>();
    let edges = chosen
        .relations
        .iter()
        .filter_map(|relation| {
            let source = layout.positions.get(&relation.source_id).copied()?;
            let target = layout.positions.get(&relation.target_id).copied()?;
            let source_issue = issue_index.get(&relation.source_id)?;
            let target_issue = issue_index.get(&relation.target_id)?;
            Some(edge(
                cx,
                relation,
                source_issue,
                target_issue,
                source,
                target,
                editable,
                &menu_kind,
                &menu_source,
                &menu_target,
            ))
        })
        .collect::<Vec<_>>();
    let graph_width = layout.width.max(1.0);
    let graph_height = layout.height.max(1.0);
    let empty = chosen.issues.is_empty();
    let empty_title = if issues.is_empty() {
        "No issues"
    } else if closed.get() {
        "No issues in this view"
    } else if canvas.get() == "linked" {
        "No linked issues"
    } else {
        "No unlinked issues"
    };
    let canvas_name = canvas.get();
    let error_message = error.clone();
    let retry_revision = revision.clone();
    let retry = view! { cx =>
        <button type="button" class="text-body-sm text-[var(--accent)]" @click=$(|_event: Event| { error_message.set("".to_owned()); retry_revision.increment(); })>"Try again"</button>
    }.boxed();
    let description = if editable {
        "Drag an issue to move it. Drag a connector to create a relation."
    } else {
        "Read-only graph. You can still pan, zoom and move issues."
    };
    let target_values = chosen
        .issues
        .iter()
        .map(|issue| {
            (
                issue.identifier.clone(),
                format!("{} — {}", issue.identifier, issue.title),
            )
        })
        .collect::<Vec<_>>();
    let target_options = target_values
        .iter()
        .map(|(identifier, label)| {
            let value = identifier.clone();
            let label = label.clone();
            view! { cx => <option value=(value)>(label)</option> }.boxed()
        })
        .collect::<Vec<_>>();
    let connect_target = menu_target.clone();
    let menu_open = menu_kind.clone();
    let menu_header = if menu_kind.get() == "edge" {
        "Manage relation"
    } else {
        "Create relation"
    };
    let target_selection = view! { cx =>
        <select aria-label="Target issue" class="rounded-md border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-body-sm" :value=$(connect_target.get()) @change=$(|event: Event| connect_target.set(event.target.value.to_owned()))>
            <option value="">"Choose an issue…"</option>
            for option in target_options { (option) }
        </select>
    }.boxed();
    let create_blocks = action_button(
        cx,
        "Create blocks relation",
        "Blocks",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "blocks",
        busy.clone(),
        error.clone(),
        revision.clone(),
        menu_kind.clone(),
    );
    let create_reverse_blocks = action_button(
        cx,
        "Create reverse blocks relation",
        "Blocked by",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "blocks_reverse",
        busy.clone(),
        error.clone(),
        revision.clone(),
        menu_kind.clone(),
    );
    let create_relates = action_button(
        cx,
        "Create relates to relation",
        "Relates to",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "relates_to",
        busy.clone(),
        error.clone(),
        revision.clone(),
        menu_kind.clone(),
    );
    let create_duplicate = action_button(
        cx,
        "Create duplicate relation",
        "Duplicate",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "duplicate",
        busy.clone(),
        error.clone(),
        revision.clone(),
        menu_kind.clone(),
    );
    let reverse_edge = action_button(
        cx,
        "Reverse relation",
        "Reverse",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "reverse",
        busy.clone(),
        error.clone(),
        revision.clone(),
        menu_kind.clone(),
    );
    let remove_edge = action_button(
        cx,
        "Remove relation",
        "Remove",
        account,
        project_id,
        menu_source.clone(),
        menu_target.clone(),
        "remove",
        busy,
        error.clone(),
        revision,
        menu_kind.clone(),
    );
    Ok(view! {
        cx =>
        <main class="native-dependency-graph flex h-full min-h-0 flex-col text-[var(--text)]" data-native-dependency-graph=(project.clone()) data-project-id=(project_id.to_string()) data-editable=(editable.to_string()) data-native-project-authority=(authority.encoded())>
            <header class="flex flex-wrap items-center gap-3 border-b border-[var(--border)] px-6 py-3">
                <div role="tablist" aria-label="Graph views" class="inline-flex gap-1 rounded-lg bg-[var(--bg-subtle)] p-1">(linked_view) (unlinked_view)</div>
                <span class="ml-auto"></span>
                (close_button)
            </header>
            <div role="alert" class="flex items-center gap-2 px-6 py-2 text-body-sm text-[var(--error)]" :hidden=$(error.get().is_empty())>$(error.get()) (retry)</div>
            <p class="sr-only">(description)</p>
            <section class="mx-6 mt-2 flex flex-wrap items-center gap-2" role="group" aria-label="Relation actions" :hidden=$(menu_kind.get().is_empty())>
                <strong class="text-body-sm">(menu_header)</strong>
                if menu_kind.get() == "connect" { (target_selection) (create_blocks) (create_reverse_blocks) (create_relates) (create_duplicate) }
                if menu_kind.get() == "edge" { (reverse_edge) (remove_edge) }
                <button type="button" class="text-body-sm text-[var(--text-muted)]" aria-label="Close relation menu" @click=$(|_event: Event| menu_open.set("".to_owned()))>"Close"</button>
            </section>
            if empty {
                <section class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center" data-native-graph-empty=(canvas_name.clone())>
                    <h2 class="text-heading font-medium">(empty_title)</h2>
                    <p class="max-w-md text-body-sm text-[var(--text-muted)]">if canvas_name == "linked" { "Create a relation between issues to see them here." } else { "Every visible issue is linked. Choose Show closed to include completed work." }</p>
                </section>
            } else {
                <section class="native-dependency-graph__viewport relative min-h-0 flex-1 overflow-hidden" data-native-graph-viewport="" data-canvas=(canvas_name)>
                    <div class="absolute left-3 top-3 z-10 flex gap-1 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-1">
                        <button type="button" aria-label="Zoom out" class="size-9 rounded-md hover:bg-[var(--bg-subtle)]" data-native-graph-zoom="out">"−"</button>
                        <button type="button" aria-label="Fit graph to view" class="size-9 rounded-md hover:bg-[var(--bg-subtle)]" data-native-graph-fit="">"⌗"</button>
                        <button type="button" aria-label="Zoom in" class="size-9 rounded-md hover:bg-[var(--bg-subtle)]" data-native-graph-zoom="in">"+"</button>
                    </div>
                    <div class="absolute inset-0 overflow-auto" style="background-image: radial-gradient(var(--border) 1px, transparent 1px); background-size: 24px 24px;">
                        <div class="relative mx-auto" data-native-graph-surface="" style=(format!("width:{}px;height:{}px;min-width:100%;min-height:100%", graph_width + 8.0, graph_height + 8.0))>
                            <svg class="pointer-events-none absolute inset-0 overflow-visible" width=(graph_width + 8.0) height=(graph_height + 8.0) viewBox=(format!("0 0 {} {}", graph_width + 8.0, graph_height + 8.0)) aria-hidden="true">
                                <defs>
                                    <marker id="native-graph-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="var(--text-faint)"></path></marker>
                                </defs>
                                for item in edges { (item) }
                            </svg>
                            for item in nodes { (item) }
                        </div>
                    </div>
                </section>
            }
            <footer class="border-t border-[var(--border)] px-6 py-2 text-caption text-[var(--text-faint)]">(description)</footer>
        </main>
    })
}

fn node<'a>(
    cx: &'a Cx,
    issue: &Issue,
    point: Point,
    editable: bool,
    menu_kind: &Signal<String>,
    menu_source: &Signal<String>,
    menu_target: &Signal<String>,
) -> BoxView<'a> {
    let href = super::super::navigation::attrs(
        cx,
        &format!(
            "/{}/issues/{}",
            issue
                .identifier
                .split_once('-')
                .map_or("", |(project, _)| project),
            issue.identifier
        ),
    );
    let style = format!(
        "position:absolute;left:{}px;top:{}px;width:200px;height:58px",
        point.x, point.y
    );
    let identifier = issue.identifier.clone();
    let issue_id = issue.id.to_string();
    let issue_title = issue.title.clone();
    let status = issue.status;
    let priority = issue.priority;
    let status_view = super::super::icons::status_icon(cx, status, 12);
    let priority_view = super::super::icons::priority_icon(cx, priority, 12);
    let issue_label = format!("{}: {}", identifier, issue_title);
    let start_connect = menu_kind.clone();
    let source_issue = menu_source.clone();
    let clear_target = menu_target.clone();
    let closed = matches!(
        issue.status,
        crate::db::models::Status::Done | crate::db::models::Status::Cancelled
    );
    let node_class = if closed {
        "native-dependency-graph__node group flex flex-col justify-center rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1.5 shadow-sm no-underline opacity-50 hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
    } else {
        "native-dependency-graph__node group flex flex-col justify-center rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1.5 shadow-sm no-underline hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
    };
    view! { cx =>
        <a class=(node_class) style=(style) data-native-graph-node=(issue_id) data-issue-id=(identifier.clone()) data-x=(point.x.to_string()) data-y=(point.y.to_string()) data-width="200" data-height="58" aria-label=(issue_label) aria-disabled="false" (href)>
            <span class="flex items-center gap-1.5"><span class="shrink-0">(status_view)</span><span class="truncate font-mono text-caption text-[var(--text-faint)]">(identifier.clone())</span><span class="ml-auto">(priority_view)</span></span>
            <span class="mt-0.5 block truncate text-caption leading-snug">(issue_title)</span>
            if editable { <button type="button" aria-label=(format!("Create relation from {}", identifier)) class="native-graph-handle absolute -right-2 top-1/2 size-2.5 -translate-y-1/2 rounded-full border-2 border-[var(--surface)] bg-[var(--accent)]" data-native-graph-connect=(identifier.clone()) @click=$(|event: Event| { event.prevent_default(); event.stop_propagation(); start_connect.set("connect".to_owned()); source_issue.set(identifier.clone()); clear_target.set("".to_owned()); })></button> }
        </a>
    }.boxed()
}

#[allow(clippy::too_many_arguments)]
fn edge<'a>(
    cx: &'a Cx,
    relation: &ProjectRelation,
    source_issue: &Issue,
    target_issue: &Issue,
    source: Point,
    target: Point,
    editable: bool,
    menu_kind: &Signal<String>,
    menu_source: &Signal<String>,
    menu_target: &Signal<String>,
) -> BoxView<'a> {
    let x1 = source.x + 200.0;
    let y1 = source.y + 29.0;
    let x2 = target.x;
    let y2 = target.y + 29.0;
    let middle = (x1 + x2) / 2.0;
    let path = format!("M {x1} {y1} C {middle} {y1}, {middle} {y2}, {x2} {y2}");
    let kind = relation.relation_type.clone();
    let stroke = "var(--text-faint)";
    let dash = match kind.as_str() {
        "blocks" => "",
        "duplicate" => "2 3",
        _ => "5 4",
    };
    let opacity = match kind.as_str() {
        "blocks" => "0.55",
        _ => "0.4",
    };
    let marker = if kind == "relates_to" {
        ""
    } else {
        "url(#native-graph-arrow)"
    };
    let source = source_issue.identifier.clone();
    let target = target_issue.identifier.clone();
    let open_manage = menu_kind.clone();
    let manage_source = menu_source.clone();
    let manage_target = menu_target.clone();
    view! { cx =>
        <g data-native-graph-edge=(format!("{}:{}:{}", source, target, kind)) data-relation-type=(kind)>
            <path d=(path) fill="none" stroke=(stroke) stroke-opacity=(opacity) stroke-width="1.5" stroke-dasharray=(dash) marker-end=(marker)></path>
        </g>
        if editable { <button type="button" class="absolute z-10 size-7 opacity-0 focus:opacity-100 hover:opacity-100" style=(format!("left:{}px;top:{}px", middle - 14.0, (y1+y2)/2.0 - 14.0)) aria-label=(format!("Manage relation from {} to {}", source, target)) data-native-graph-manage=(format!("{}:{}",source,target)) @click=$(|event: Event| {event.prevent_default();open_manage.set("edge".to_owned());manage_source.set(source.clone());manage_target.set(target.clone());})></button> }
    }.boxed()
}

#[allow(clippy::too_many_arguments)]
fn action_button<'a>(
    cx: &'a Cx,
    aria: &'static str,
    label: &'static str,
    account: i64,
    project_id: i64,
    source: Signal<String>,
    target: Signal<String>,
    action: &'static str,
    busy: Signal<bool>,
    error: Signal<String>,
    revision: Signal<usize>,
    menu: Signal<String>,
) -> BoxView<'a> {
    let busy_disabled = busy.clone();
    let source_disabled = source.clone();
    let target_disabled = target.clone();
    let busy_action = busy.clone();
    let busy_success = busy.clone();
    let busy_failure = busy;
    let error_action = error.clone();
    let error_failure = error;
    let revision_action = revision;
    let menu_action = menu;
    let source_action = source;
    let target_action = target;
    view! { cx =>
        <button type="button" aria-label=(aria) class="rounded-md border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1.5 text-body-sm disabled:opacity-50" :disabled=$(if busy_disabled.get() { true } else { if source_disabled.get().is_empty() { true } else { target_disabled.get().is_empty() } }) @click=$(async |_event: Event| {
            busy_action.set(true); error_action.set("".to_owned());
            let _success = |_message: StringSurrogate| { busy_success.set(false); menu_action.set("".to_owned()); revision_action.increment(); };
            let _failure = || { busy_failure.set(false); error_failure.set("Couldn't update the relation. Check your access and try again.".to_owned()); };
            let source_value=source_action.get(); let target_value=target_action.get();
            let _run = async || {
                if action == "reverse" { reverse_relation(account,project_id,source_value.clone(),target_value.clone()).await }
                else if action == "remove" { unlink_relation(account,project_id,source_value.clone(),target_value.clone()).await }
                else if action == "blocks_reverse" { link_relation(account,project_id,target_value.clone(),source_value.clone(),"blocks".to_owned()).await }
                else { let kind = if action == "blocks" { "blocks" } else if action == "duplicate" { "duplicate" } else { "relates_to" }; link_relation(account,project_id,source_value.clone(),target_value.clone(),kind.to_owned()).await }
            };
            raw!("Promise.resolve().then(() => ${_run}()).then(value => ${_success}(cx.hydrate(value))).catch(() => ${_failure}());",());
        })>(label)</button>
    }.boxed()
}
fn edges(relations: &[ProjectRelation]) -> Vec<RelationEdge> {
    relations
        .iter()
        .map(|relation| RelationEdge {
            source: relation.source_id,
            target: relation.target_id,
            kind: match relation.relation_type.as_str() {
                "blocks" => RelationKind::Blocks,
                "duplicate" => RelationKind::Duplicate,
                _ => RelationKind::RelatesTo,
            },
        })
        .collect()
}
fn linked_count(issues: &[Issue], relations: &[ProjectRelation], closed: bool) -> usize {
    model::project(issues, relations, closed).counts.linked
}
fn unlinked_count(issues: &[Issue], relations: &[ProjectRelation], closed: bool) -> usize {
    model::project(issues, relations, closed).counts.unlinked
}
