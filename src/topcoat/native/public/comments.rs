//! Bounded, retained comment pages for published issue and page threads.
use super::super::super::runtime::signal_vec::{SignalVecExt, VecPositionExt};
use super::super::icons::{self, UiIcon};
use super::super::{browser, dates};
use super::data;
use crate::db::{
    models::Comment,
    queries::comments::{CommentCursor, CommentPage, CommentParent},
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

type ThreadState = (
    Signal<i64>,
    Signal<Vec<i64>>,
    Signal<usize>,
    Signal<usize>,
    Signal<usize>,
    Signal<usize>,
    Signal<i64>,
    Signal<i64>,
    Signal<bool>,
);
type PageControl = (Signal<usize>, Signal<bool>, Signal<bool>, Signal<String>);

use comment_page_shard::public_comment_page;

struct ThreadContext<'a> {
    project: &'a str,
    parent_kind: &'a str,
    parent_id: i64,
    state: ThreadState,
}

const AUTO_PAGE_LIMIT: usize = 5;

pub(super) fn thread<'a>(
    cx: &'a Cx,
    project: &str,
    parent: CommentParent,
    initial: &CommentPage,
) -> BoxView<'a> {
    let (parent_kind, parent_id) = match parent {
        CommentParent::Issue(id) => ("issue", id),
        CommentParent::Page(id) => ("page", id),
    };
    let project = project.to_owned();
    let owner = cx.keyed(("public-comments", parent_kind, parent_id));
    let initial_ids = initial
        .items
        .iter()
        .map(|comment| comment.id)
        .collect::<Vec<_>>();
    let target_query = topcoat::router::request::uri(cx)
        .query()
        .and_then(query_id)
        .unwrap_or(-1);
    let state = (
        signal(&owner, || target_query),
        signal(&owner, move || initial_ids),
        signal(&owner, || initial.items.len()),
        signal(&owner, || 1_usize),
        signal(&owner, || 0_usize),
        signal(&owner, || 0_usize),
        signal(&owner, || target_query),
        signal(&owner, || -1_i64),
        signal(&owner, || initial.has_more),
    );
    let rows = if initial.items.is_empty() && !initial.has_more {
        view! {
            owner =>
            <div
                class="flex flex-col items-center justify-center gap-2 py-8 text-caption text-[var(--text-muted)]"
            >
                (icons::ui_icon(cx, UiIcon::Comment, 22))
                <p>"No comments yet"</p>
            </div>
        }
        .boxed()
    } else {
        segment(
            cx,
            ThreadContext {
                project: &project,
                parent_kind,
                parent_id,
                state: state.clone(),
            },
            initial.clone(),
            0,
            None,
            Vec::new(),
        )
    };
    let visible_count = state.2.clone();
    let has_more = state.8;
    view! {
        owner =>
        <section
            class="flex flex-col gap-3"
            data-public-comments=""
            :data-native-public-visible-count=$(visible_count.get())
        >
            <header class="flex items-center gap-2">
                <h2 class="text-heading">"Comments"</h2>
                <span
                    class="text-caption text-[var(--text-muted)]"
                    :hidden=$(visible_count.get() == 0_usize)
                    :title=$({
                        let count = visible_count.get();
                        if has_more.get() {
                            raw!(
                                "cx.hydrate(${count}.toString()+' newest shown, older comments not loaded yet')",
                                format!(
                                    "{count} newest shown, older comments not loaded yet",
                                ),
                            )
                        } else {
                            raw!(
                                "cx.hydrate(${count}.toString()+' comments')",
                                format!("{count} comments"),
                            )
                        }
                    })
                >
                    $(visible_count.get())
                    <span :hidden=$(!has_more.get())>"+"</span>
                </span>
            </header>
            (rows)
        </section>
    }
    .boxed()
}

fn segment<'a>(
    cx: &'a Cx,
    context: ThreadContext<'_>,
    page: CommentPage,
    depth: usize,
    completion: Option<PageControl>,
    added_ids: Vec<i64>,
) -> BoxView<'a> {
    let ThreadContext {
        project,
        parent_kind,
        parent_id,
        state,
    } = context;
    let cursor = page.items.last().map(|oldest| CommentCursor {
        created_at: oldest.created_at.clone(),
        id: oldest.id,
    });
    let mut comments = page.items;
    comments.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then(left.id.cmp(&right.id))
    });
    let rows = comments
        .into_iter()
        .map(|comment| comment_row(cx, project, comment))
        .collect::<Vec<_>>();
    let page_owner = cx.keyed(("public-comment-segment", parent_kind, parent_id, depth));
    let older_control = if page.has_more {
        if let Some(cursor) = cursor {
            let request = signal(&page_owner, || 0_usize);
            let busy = signal(&page_owner, || false);
            let loaded = signal(&page_owner, || false);
            let error = signal(&page_owner, String::new);
            let control: PageControl =
                (request.clone(), busy.clone(), loaded.clone(), error.clone());
            let before_created_at = cursor.created_at.clone();
            let before_id = cursor.id;
            let project_for_page = project.to_owned();
            let kind_for_page = parent_kind.to_owned();
            let page_state = state.clone();
            let revision_request = request.clone();
            let child_control = control.clone();
            let child = view! {
                page_owner =>
                public_comment_page(
                    project: project_for_page,
                    parent_kind: kind_for_page,
                    parent_id: parent_id,
                    before_created_at: before_created_at,
                    before_id: before_id,
                    depth: depth + 1,
                    revision: $(revision_request.get()),
                    control: child_control,
                    state: page_state
                )
            }
            .boxed();
            let click_request = request;
            let click_busy = busy.clone();
            let click_error = error.clone();
            let click_loaded = loaded.clone();
            let click_browser = browser::bindings();
            let click = expr!(|_event: Event| {
                if !click_browser.is_disposed() {
                    if !click_busy.get() {
                        if !click_loaded.get() {
                            click_busy.set(true);
                            click_error.set("".to_owned());
                            click_request.set(click_request.get() + 1_usize);
                        }
                    }
                }
            });
            let mount = target_mount(
                cx,
                depth,
                page.has_more,
                control,
                state,
                completion,
                added_ids,
            );
            let mut mount_attributes = Attributes::with_capacity(2);
            mount_attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
            mount_attributes.insert(cx, "data-native-public-comment-segment", depth.to_string());
            let older = view! {
                page_owner =>
                <div class="contents" (mount_attributes)>
                    <div class="flex flex-col items-start gap-2">
                        <button
                            type="button"
                            class="rounded-md px-3 py-2 text-caption text-[var(--accent)] hover:bg-[var(--bg-subtle)]"
                            data-native-public-load-older=""
                            :hidden=$(loaded.get())
                            :disabled=$(busy.get())
                            :aria-busy=$(if busy.get() { "true" } else { "false" })
                            data-topcoat-on:click=(click.into_evaluated_and_js().1)
                        >
                            $({
                                if busy.get() {
                                    "Loading older comments…"
                                } else {
                                    "Load older comments"
                                }
                            })
                        </button>
                        <p
                            role="alert"
                            class="text-caption text-[var(--danger)]"
                            :hidden=$(error.get().is_empty())
                        >
                            $(error.get())
                        </p>
                    </div>
                    (child)
                </div>
            }
            .boxed();
            Some(older)
        } else {
            None
        }
    } else {
        let mount = target_mount(
            cx,
            depth,
            false,
            (
                signal(cx, || 0_usize),
                signal(cx, || false),
                signal(cx, || false),
                signal(cx, String::new),
            ),
            state,
            completion,
            added_ids,
        );
        let mut mount_attributes = Attributes::with_capacity(2);
        mount_attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
        mount_attributes.insert(cx, "data-native-public-comment-segment", depth.to_string());
        Some(view! { cx => <span (mount_attributes)></span> }.boxed())
    };
    view! {
        page_owner =>
        if let Some(older) = older_control {
            (older)
        }
        <ol class="m-0 list-none p-0">
            for row in rows {
                (row)
            }
        </ol>
    }
    .boxed()
}

fn target_mount(
    cx: &Cx,
    depth: usize,
    has_more: bool,
    control: PageControl,
    state: ThreadState,
    completion: Option<PageControl>,
    added_ids: Vec<i64>,
) -> topcoat::runtime::Expr<impl FnOnce(Event)> {
    let browser = browser::bindings();
    let render_path = super::super::transport::mounted_url(cx, "/public/__native/comments");
    let (request, busy, _, error) = control;
    let (
        target,
        loaded_ids,
        loaded_count,
        loaded_pages,
        attempts,
        last_count,
        previous_target,
        scrolled,
        thread_has_more,
    ) = state;
    let (_, control_busy, control_loaded, control_error) = completion.unwrap_or_else(|| {
        (
            signal(cx, || 0_usize),
            signal(cx, || false),
            signal(cx, || false),
            signal(cx, || "".to_owned()),
        )
    });
    let loaded_ids_for_page = loaded_ids.clone();
    let loaded_count_for_page = loaded_count.clone();
    let loaded_pages_for_page = loaded_pages.clone();
    let thread_has_more_for_page = thread_has_more;
    let control_busy_for_page = control_busy;
    let control_loaded_for_page = control_loaded;
    let control_error_for_page = control_error;
    let target_for_reveal = target;
    let loaded_ids_for_reveal = loaded_ids;
    let loaded_count_for_reveal = loaded_count;
    let loaded_pages_for_reveal = loaded_pages;
    let attempts_for_reveal = attempts;
    let last_count_for_reveal = last_count;
    let previous_target_for_reveal = previous_target;
    let scrolled_for_reveal = scrolled;
    let busy_for_reveal = busy.clone();
    let error_for_reveal = error.clone();
    let request_for_reveal = request;
    let busy_for_failure = busy.clone();
    let error_for_failure = error;
    let iteration_cx = cx.keyed(("public-comment-page-mount", depth));
    let add_index = signal(&iteration_cx, || 0_usize);
    let added_comment_ids = signal(&iteration_cx, move || added_ids);
    expr!(|_event: Event| {
        let page_loaded = || {
            if !browser.is_disposed() {
                control_busy_for_page.set(false);
                control_loaded_for_page.set(true);
                control_error_for_page.set("".to_owned());
                add_index.set(0_usize);
                while add_index.get() < added_comment_ids.get().len() {
                    let comment_id = *added_comment_ids.get().index(add_index.get());
                    if loaded_ids_for_page.get().position(comment_id).is_none() {
                        loaded_ids_for_page.push(comment_id);
                    }
                    add_index.increment();
                }
                loaded_count_for_page.set(loaded_ids_for_page.get().len());
                if depth + 1_usize > loaded_pages_for_page.get() {
                    loaded_pages_for_page.set(depth + 1_usize);
                    thread_has_more_for_page.set(has_more);
                }
            }
        };
        let reveal = |_event: Event| {
            if !browser.is_disposed() {
                let requested_target = browser.positive_i64(
                    raw!("(()=>{const u=new URL(window.location.href);return u.hash.startsWith('#comment-')?u.hash.slice(9):u.searchParams.get('comment')||''})()", "".to_owned()),
                    -1_i64,
                );
                if previous_target_for_reveal.get() != requested_target {
                    previous_target_for_reveal.set(requested_target);
                    attempts_for_reveal.set(0_usize);
                    last_count_for_reveal.set(0_usize);
                    scrolled_for_reveal.set(-1_i64);
                }
                target_for_reveal.set(requested_target);
                if requested_target > 0_i64 {
                    if loaded_ids_for_reveal
                        .get()
                        .position(requested_target)
                        .is_some()
                    {
                        if scrolled_for_reveal.get() != requested_target {
                            scrolled_for_reveal.set(requested_target);
                            browser.microtask(|| {
                                if !browser.is_disposed() {
                                    let current_target = browser.positive_i64(
                                        raw!("(()=>{const u=new URL(window.location.href);return u.hash.startsWith('#comment-')?u.hash.slice(9):u.searchParams.get('comment')||''})()", "".to_owned()),
                                        -1_i64,
                                    );
                                    if current_target == requested_target {
                                        browser.scroll_selector(raw!(
                                            "'#comment-'+${requested_target}.toString()",
                                            "".to_owned()
                                        ));
                                    }
                                }
                            });
                        }
                    } else if has_more {
                        if !busy.get() {
                            if depth + 1_usize == loaded_pages_for_reveal.get() {
                                if attempts_for_reveal.get() < AUTO_PAGE_LIMIT {
                                    if last_count_for_reveal.get() != loaded_count_for_reveal.get()
                                    {
                                        last_count_for_reveal.set(loaded_count_for_reveal.get());
                                        attempts_for_reveal
                                            .set(attempts_for_reveal.get() + 1_usize);
                                        busy_for_reveal.set(true);
                                        error_for_reveal.set("".to_owned());
                                        request_for_reveal.set(request_for_reveal.get() + 1_usize);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        browser.call0(page_loaded);
        raw!("${reveal}(${_event});", ());
        browser.window_listener("hashchange".to_owned(), reveal);
        let _failed = |path: String| {
            if !browser.is_disposed() {
                if path == render_path {
                    if busy_for_failure.get() {
                        busy_for_failure.set(false);
                        error_for_failure
                            .set("Comments could not be loaded. Try again.".to_owned());
                    }
                }
            }
        };
        raw!(
            "const owner=${_event}.inner.target; owner.addEventListener('topcoat:render-error',event=>${_failed}(cx.hydrate(event.detail.path)),{signal:cx.abortSignal});",
            ()
        );
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat generates a request handler for the retained page shard"
)]
mod comment_page_shard {
    use super::*;

    #[shard("/public/__native/comments")]
    pub(super) async fn public_comment_page(
        cx: &Cx,
        project: String,
        parent_kind: String,
        parent_id: i64,
        before_created_at: String,
        before_id: i64,
        depth: usize,
        revision: usize,
        control: PageControl,
        state: ThreadState,
    ) -> topcoat::Result<impl View> {
        if depth == 0 || depth.checked_add(1).is_none() {
            return Err(topcoat::router::error::not_found().into());
        }
        if revision == 0 {
            return Ok(view! { cx => <span></span> }.boxed());
        }
        let parent = match parent_kind.as_str() {
            "issue" => CommentParent::Issue(parent_id),
            "page" => CommentParent::Page(parent_id),
            _ => return Err(topcoat::router::error::not_found().into()),
        };
        let before = CommentCursor {
            created_at: before_created_at,
            id: before_id,
        };
        let page = super::super::super::session::read(
            cx,
            data::read_comments(cx, &project, parent, Some(&before)),
        )?;
        let ids = page
            .items
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>();
        Ok(segment(
            cx,
            ThreadContext {
                project: &project,
                parent_kind: if matches!(parent, CommentParent::Issue(_)) {
                    "issue"
                } else {
                    "page"
                },
                parent_id,
                state,
            },
            page,
            depth,
            Some(control),
            ids,
        ))
    }
}

fn comment_row<'a>(cx: &'a Cx, project: &str, comment: Comment) -> BoxView<'a> {
    let comment_cx = cx.keyed(("public-comment", comment.id));
    let project = project.to_owned();
    view! { comment_cx => public_comment(project: project, comment: comment) }.boxed()
}

#[component]
async fn public_comment(cx: &Cx, project: String, comment: Comment) -> topcoat::Result<impl View> {
    let author = if comment.author_display_name.is_empty() {
        "Someone".to_owned()
    } else {
        comment.author_display_name.clone()
    };
    let created = dates::absolute_time_view(cx, &comment.created_at);
    let edited = comment.updated_at > comment.created_at;
    let id = comment.id.to_string();
    let target = format!("comment-{id}");
    let anchor_text = format!("#{id}");
    let comment_href = format!("#comment-{id}");
    let updated_at = comment.updated_at.clone();
    let content = super::markdown_view(cx, &project, &comment.content);
    let verification = comment.kind == crate::db::models::CommentKind::Verification;
    Ok(view! {
        cx =>
        <li id=(target) class="border-b border-solid border-[var(--border)] py-4">
            <header class="mb-2 flex flex-wrap items-center gap-2 text-caption">
                <a href=(comment_href) class="font-mono text-[var(--text-faint)]">
                    (anchor_text)
                </a>
                <strong>(author)</strong>
                (created)
                if edited {
                    <span class="text-[var(--text-faint)]" title=(updated_at)>
                        "edited"
                    </span>
                }
                if verification {
                    <span
                        class="rounded bg-[var(--bg-subtle)] px-1.5 py-0.5 text-[var(--text-muted)]"
                        title="Evidence recorded when this issue was closed"
                    >
                        "Verification"
                    </span>
                }
            </header>
            <div class="tc-markdown">(content)</div>
        </li>
    })
}

pub(super) fn query_id(query: &str) -> Option<i64> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("comment="))?
        .parse()
        .ok()
}

#[cfg(test)]
pub(super) fn linked_comment_id(href: &str) -> Option<i64> {
    href.split_once('#')?
        .1
        .strip_prefix("comment-")?
        .parse()
        .ok()
}
