//! Read-only native views for published issues and pages.
use super::super::super::runtime::signal_vec::VecPositionExt;
use super::super::{
    breadcrumbs::{self, Segment},
    navigation, transport,
};
use super::data::{Body, Snapshot};
use crate::db::models::{Attachment, Comment, Issue, Page};
use topcoat::{
    context::Cx,
    runtime::{Event, expr, signal},
    view::{BoxView, View, ViewExt, component, view},
};

pub(super) fn content<'a>(cx: &'a Cx, snapshot: &Snapshot) -> BoxView<'a> {
    match &snapshot.body {
        Body::Issues(collection) => super::collection::content(
            cx,
            collection,
            matches!(
                snapshot.route,
                super::super::public_route::Route::Board { .. }
            ),
        ),
        Body::Issue {
            issue,
            comments,
            attachments,
            modules,
            labels,
        } => {
            let issue_cx = cx.keyed(format!("public-issue:{}", issue.id));
            let project = snapshot.project.identifier.clone();
            let issue = issue.clone();
            let comments = comments.clone();
            let attachments = attachments.clone();
            let modules = modules.clone();
            let labels = labels.clone();
            view! {
                issue_cx =>
                issue_detail_component(
                    project: project,
                    issue: issue,
                    comments: comments,
                    attachments: attachments,
                    modules: modules,
                    labels: labels
                )
            }
            .boxed()
        }
        Body::Pages { folders, .. } => super::pages::content(cx, &snapshot.project, folders),
        Body::Page {
            page,
            comments,
            attachments,
            folders,
        } => {
            let page_cx = cx.keyed(format!("public-page:{}", page.id));
            let project_id = snapshot.project.id;
            let project = snapshot.project.identifier.clone();
            let page = page.clone();
            let comments = comments.clone();
            let attachments = attachments.clone();
            let folders = folders.clone();
            view! {
                page_cx =>
                page_detail_component(
                    project_id: project_id,
                    project: project,
                    page: page,
                    comments: comments,
                    attachments: attachments,
                    folders: folders
                )
            }
            .boxed()
        }
    }
}

#[component]
async fn issue_detail_component(
    cx: &Cx,
    project: String,
    issue: Issue,
    comments: Vec<Comment>,
    attachments: Vec<Attachment>,
    modules: Vec<crate::db::models::Module>,
    labels: Vec<crate::db::models::Label>,
) -> topcoat::Result<impl View> {
    Ok(issue_detail(
        cx,
        &project,
        &issue,
        &comments,
        &attachments,
        &modules,
        &labels,
    ))
}

#[component]
async fn page_detail_component(
    cx: &Cx,
    project_id: i64,
    project: String,
    page: Page,
    comments: Vec<Comment>,
    attachments: Vec<Attachment>,
    folders: Vec<crate::db::models::Folder>,
) -> topcoat::Result<impl View> {
    Ok(page_detail(
        cx,
        project_id,
        &project,
        &page,
        &comments,
        &attachments,
        &folders,
    ))
}

fn issue_detail<'a>(
    cx: &'a Cx,
    project: &str,
    issue: &Issue,
    comments: &[Comment],
    attachments: &[Attachment],
    modules: &[crate::db::models::Module],
    labels: &[crate::db::models::Label],
) -> BoxView<'a> {
    let identifier = issue.identifier.clone();
    let title = issue.title.clone();
    let status = issue.status.as_str().to_owned();
    let priority = issue.priority.as_str().to_owned();
    let description = super::markdown_view(cx, project, &issue.description);
    let module = modules
        .iter()
        .find(|module| Some(module.id) == issue.module_id)
        .map(|module| module.name.clone())
        .unwrap_or_default();
    let label_names = labels
        .iter()
        .filter(|label| issue.labels.contains(&label.name))
        .map(|label| label.name.clone())
        .collect::<Vec<_>>();
    let created = super::super::dates::absolute_time_view(cx, &issue.created_at);
    let updated = super::super::dates::absolute_time_view(cx, &issue.updated_at);
    let relations = relation_groups(cx, project, issue);
    let comments = comments_view(cx, project, comments);
    let attachments = attachments_view(cx, project, attachments);
    let back = navigation::attrs(cx, &format!("/public/{project}/issues"));
    view! {
        cx =>
        <main
            class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 py-6 md:px-8"
            data-public-issue=(identifier.clone())
        >
            <a (back) class="text-caption text-[var(--text-muted)]">"Back to issues"</a>
            <header class="border-b border-solid border-[var(--border)] pb-5">
                <div
                    class="flex flex-wrap items-center gap-2 text-caption text-[var(--text-muted)]"
                >
                    <span>(identifier)</span>
                    <span>(status)</span>
                    <span>(priority)</span>
                </div>
                <h1 class="mt-2 text-title font-semibold">(title)</h1>
                <span
                    class="inline-flex w-fit rounded-full bg-[var(--bg-subtle)] px-2 py-1 text-micro text-[var(--text-muted)]"
                    title="Read-only. This is the public view of the project; sign in to edit or comment."
                >
                    "Read-only"
                </span>
                if !module.is_empty() {
                    <p class="text-body-sm text-[var(--text-muted)]">
                        "Module: "
                        (module)
                    </p>
                }
                <div class="flex flex-wrap gap-2">
                    for label in label_names {
                        <span
                            class="rounded bg-[var(--bg-subtle)] px-2 py-1 text-caption"
                        >
                            (label)
                        </span>
                    }
                </div>
            </header>
            <div class="grid gap-5 md:grid-cols-[minmax(0,1fr)_14rem]">
                <article class="tc-markdown min-w-0" data-public-markdown="">
                    (description)
                </article>
                <aside
                    class="flex flex-col gap-4 border-t border-solid border-[var(--border)] pt-4 md:border-l md:border-t-0 md:pl-4 md:pt-0"
                >
                    <div class="text-caption">
                        <strong>"Created"</strong>
                        <div class="text-[var(--text-muted)]">(created)</div>
                    </div>
                    <div class="text-caption">
                        <strong>"Updated"</strong>
                        <div class="text-[var(--text-muted)]">(updated)</div>
                    </div>
                    (relations)
                </aside>
            </div>
            (attachments)
            <section class="border-t border-solid border-[var(--border)] pt-5">
                <h2 class="text-heading">"Activity"</h2>
                (comments)
            </section>
        </main>
    }.boxed()
}

fn page_detail<'a>(
    cx: &'a Cx,
    project_id: i64,
    project: &str,
    page: &Page,
    comments: &[Comment],
    attachments: &[Attachment],
    folders: &[crate::db::models::Folder],
) -> BoxView<'a> {
    let title = page.title.clone();
    let identifier = page.identifier.clone();
    let page_id = page.id.to_string();
    let status = page.status.clone();
    let content = super::markdown_view(cx, project, &page.content);
    let comments = comments_view(cx, project, comments);
    let attachments = attachments_view(cx, project, attachments);
    let created = super::super::dates::absolute_time_view(cx, &page.created_at);
    let updated = super::super::dates::absolute_time_view(cx, &page.updated_at);
    let labels = page.labels.clone();
    let back = navigation::attrs(cx, &format!("/public/{project}/pages"));
    let list_path = format!("/public/{project}/pages");
    let mut folder_path = Vec::new();
    let mut next_folder = page.folder_id;
    while let Some(folder_id) = next_folder {
        let Some(folder) = folders.iter().find(|folder| folder.id == folder_id) else {
            break;
        };
        folder_path.push(folder.name.clone());
        next_folder = folder.parent_id;
    }
    folder_path.reverse();
    let mut segments = vec![
        Segment {
            content: breadcrumbs::link(cx, project, &list_path, true),
            hide_below_sm: true,
            copy: Some(project.to_owned()),
        },
        Segment {
            content: breadcrumbs::link(cx, "Pages", &list_path, false),
            hide_below_sm: true,
            copy: None,
        },
    ];
    for folder in folder_path {
        segments.push(Segment {
            content: breadcrumbs::link(cx, &folder, &list_path, false),
            hide_below_sm: false,
            copy: None,
        });
    }
    segments.push(Segment {
        content: breadcrumbs::current(cx, &identifier, true),
        hide_below_sm: false,
        copy: Some(identifier.clone()),
    });
    let breadcrumb = breadcrumbs::render(cx, project_id, segments);
    view! {
        cx =>
        <main
            class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 py-6 md:px-8"
            data-public-page=(page_id)
        >
            (breadcrumb)
            <a (back) class="text-caption text-[var(--text-muted)]">"Back to pages"</a>
            <header class="border-b border-solid border-[var(--border)] pb-5">
                <p class="text-caption text-[var(--text-muted)]">
                    <span>(identifier)</span>
                    <span>" / "</span>
                    <span>(status)</span>
                </p>
                <h1 class="text-title font-semibold">(title)</h1>
                <span
                    class="inline-flex rounded-full bg-[var(--bg-subtle)] px-2 py-1 text-micro text-[var(--text-muted)]"
                    title="Read-only. This is the public view of the project; sign in to edit or comment."
                >
                    "Read-only"
                </span>
                <div class="mt-3 flex flex-wrap gap-2">
                    for label in labels {
                        <span
                            class="rounded bg-[var(--bg-subtle)] px-2 py-1 text-caption"
                        >
                            (label)
                        </span>
                    }
                </div>
            </header>
            <article class="tc-markdown min-w-0" data-public-markdown="">
                (content)
            </article>
            <footer
                class="flex flex-wrap gap-8 border-t border-solid border-[var(--border)] pt-5 text-caption"
            >
                <div>
                    <strong>"Created"</strong>
                    <div class="text-[var(--text-muted)]">(created)</div>
                </div>
                <div>
                    <strong>"Updated"</strong>
                    <div class="text-[var(--text-muted)]">(updated)</div>
                </div>
            </footer>
            (attachments)
            <section class="border-t border-solid border-[var(--border)] pt-5">
                <h2 class="text-heading">"Comments"</h2>
                (comments)
            </section>
        </main>
    }.boxed()
}

fn relation_groups<'a>(cx: &'a Cx, project: &str, issue: &Issue) -> BoxView<'a> {
    let groups = [
        ("Blocks", &issue.blocks),
        ("Blocked by", &issue.blocked_by),
        ("Related", &issue.relates_to),
        ("Duplicates", &issue.duplicates),
        ("Duplicated by", &issue.duplicated_by),
    ]
    .into_iter()
    .filter(|(_, issues)| !issues.is_empty())
    .map(|(label, issues)| {
        let rows = issues
            .iter()
            .map(|identifier| {
                let href = navigation::attrs(cx, &format!("/public/{project}/issues/{identifier}"));
                let identifier = identifier.clone();
                view! {
                    cx =>
                    <a class="font-mono text-caption hover:underline" (href)>
                        (identifier)
                    </a>
                }
                .boxed()
            })
            .collect::<Vec<_>>();
        view! {
            cx =>
            <div class="text-caption">
                <strong class="mb-1 block">(label)</strong>
                <div class="flex flex-wrap gap-2">
                    for row in rows {
                        (row)
                    }
                </div>
            </div>
        }
        .boxed()
    })
    .collect::<Vec<_>>();
    view! {
        cx =>
        <section class="flex flex-col gap-3" aria-label="Issue relations">
            for group in groups {
                (group)
            }
        </section>
    }
    .boxed()
}

fn comments_view<'a>(cx: &'a Cx, project: &str, comments: &[Comment]) -> BoxView<'a> {
    let mut ordered = comments.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then(left.id.cmp(&right.id))
    });
    let total = ordered.len();
    let request_uri = topcoat::router::request::uri(cx);
    let linked_comment = request_uri.query().and_then(linked_comment_query_id);
    let initial_visible = ordered
        .iter()
        .position(|comment| Some(comment.id) == linked_comment)
        .map_or_else(|| total.min(50), |index| (total - index).max(50).min(total));
    let visible_count = signal(cx, || initial_visible);
    let load_more = visible_count.clone();
    let load_older = expr!(|_event: Event| {
        if load_more.get() < total {
            let next = load_more.get() + 50_usize;
            load_more.set(if next > total { total } else { next });
        }
    });
    let target_ids = ordered.iter().map(|comment| comment.id).collect::<Vec<_>>();
    let browser = super::super::browser::bindings();
    let mount = expr!(|_event: Event| {
        let reveal = |_event: Event| {
            if !browser.is_disposed() {
                let target = browser.positive_i64(
                    raw!("(()=>{const u=new URL(window.location.href);return u.hash.startsWith('#comment-')?u.hash.slice(9):u.searchParams.get('comment')||''})()", "".to_owned()),
                    -1_i64,
                );
                let index = target_ids.clone().position(target);
                if index.is_some() {
                    let count = total - index.unwrap();
                    if visible_count.get() < count {
                        visible_count.set(count);
                    }
                    browser.microtask(|| {
                        browser.scroll_selector(raw!(
                            "'#comment-'+${target}.toString()",
                            "".to_owned()
                        ));
                    });
                }
            }
        };
        reveal(_event);
        browser.window_listener("hashchange".to_owned(), reveal);
    });
    let mut reveal_attributes = topcoat::view::Attributes::with_capacity(1);
    reveal_attributes.insert(cx, "data-topcoat-on:mount", mount.into_evaluated_and_js().1);
    let rows = ordered
        .iter()
        .enumerate()
        .map(|(index, comment)| {
            comment_row(cx, project, comment, visible_count.clone(), total - index)
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div
            data-public-comments=""
            :data-native-public-visible-count=$(visible_count.get())
        >
            <span hidden="hidden" (reveal_attributes)></span>
            <button
                type="button"
                class="mb-3 rounded-md px-3 py-2 text-caption text-[var(--accent)] hover:bg-[var(--bg-subtle)]"
                :hidden=$(if load_more.get() >= total { true } else { false })
                data-topcoat-on:click=(load_older.into_evaluated_and_js().1)
            >
                "Load older comments"
            </button>
            <ol class="list-none p-0">
                for row in rows {
                    (row)
                }
            </ol>
        </div>
    }.boxed()
}

fn comment_row<'a>(
    cx: &'a Cx,
    project: &str,
    comment: &Comment,
    visible_count: topcoat::runtime::Signal<usize>,
    remaining_from_end: usize,
) -> BoxView<'a> {
    let comment_cx = cx.keyed(("public-comment", comment.id));
    let project = project.to_owned();
    let comment = comment.clone();
    view! {
        comment_cx =>
        public_comment(
            project: project,
            comment: comment,
            visible_count: visible_count,
            remaining_from_end: remaining_from_end
        )
    }
    .boxed()
}

#[component]
async fn public_comment(
    cx: &Cx,
    project: String,
    comment: Comment,
    visible_count: topcoat::runtime::Signal<usize>,
    remaining_from_end: usize,
) -> topcoat::Result<impl View> {
    let author = if comment.author_display_name.is_empty() {
        "Someone".to_owned()
    } else {
        comment.author_display_name.clone()
    };
    let created = super::super::dates::absolute_time_view(cx, &comment.created_at);
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
        <li
            id=(target)
            class="border-b border-solid border-[var(--border)] py-4"
            :hidden=$(if visible_count.get() < remaining_from_end {
                true
            } else {
                false
            })
        >
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

#[cfg(test)]
pub(super) fn linked_comment_id(href: &str) -> Option<i64> {
    href.split_once('#')?
        .1
        .strip_prefix("comment-")?
        .parse()
        .ok()
}

pub(super) fn linked_comment_query_id(query: &str) -> Option<i64> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("comment="))?
        .parse()
        .ok()
}

fn attachments_view<'a>(cx: &'a Cx, project: &str, attachments: &[Attachment]) -> BoxView<'a> {
    let rows = attachments
        .iter()
        .map(|attachment| {
            let url = transport::mounted_url(
                cx,
                &format!(
                    "/public/api/projects/{project}/attachments/{}",
                    attachment.id
                ),
            );
            let filename = attachment.filename.clone();
            let mime = attachment.mime.clone();
            let alt = attachment
                .alt_text
                .clone()
                .unwrap_or_else(|| filename.clone());
            view! {
                cx =>
                <figure class="my-4">
                    if mime.starts_with("image/") {
                        <img
                            src=(url.clone())
                            alt=(alt)
                            class="max-h-[40rem] max-w-full rounded-md object-contain"
                            loading="lazy"
                        />
                    } else if mime.starts_with("audio/") {
                        <audio controls=(true) src=(url.clone())>
                            (filename.clone())
                        </audio>
                    } else if mime.starts_with("video/") {
                        <video controls=(true) class="max-w-full" src=(url.clone())>
                            (filename.clone())
                        </video>
                    } else {
                        <a href=(url)>(filename.clone())</a>
                    }
                    <figcaption class="text-caption text-[var(--text-muted)]">
                        (filename)
                    </figcaption>
                </figure>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <section aria-label="Attachments">
            for row in rows {
                (row)
            }
        </section>
    }
    .boxed()
}
