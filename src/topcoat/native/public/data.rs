use std::collections::HashMap;

use topcoat::context::Cx;

use crate::{
    db::{
        models::{
            Attachment, AttachmentEntity, Comment, Folder, Issue, Label, ListIssuesQuery, Module,
            Page, Project,
        },
        queries::{self, comments::CommentParent, public as q},
    },
    error::LificError,
    services::issues::IssueCollection,
};

use super::super::public_route::Route;

pub(super) struct Snapshot {
    pub project: Project,
    pub route: Route,
    pub body: Body,
}

pub(super) enum Body {
    Issues(IssueCollection),
    Pages {
        pages: Vec<Page>,
        folders: Vec<Folder>,
    },
    Issue {
        issue: Issue,
        comments: Vec<Comment>,
        attachments: Vec<Attachment>,
        modules: Vec<Module>,
        labels: Vec<Label>,
    },
    Page {
        page: Page,
        comments: Vec<Comment>,
        attachments: Vec<Attachment>,
        folders: Vec<Folder>,
    },
}

pub(super) fn load(cx: &Cx, route: &Route) -> Result<Snapshot, LificError> {
    let project_identifier = match route {
        Route::Redirect(_) => return Err(not_found()),
        Route::Issues { project }
        | Route::Board { project }
        | Route::IssueDetail { project, .. }
        | Route::Pages { project }
        | Route::PageDetail { project, .. } => project,
    };

    super::super::context::with_published(cx, project_identifier, |conn, project| {
        let body = match route {
            Route::Issues { .. } | Route::Board { .. } => {
                Body::Issues(issue_collection(conn, project)?)
            }
            Route::Pages { .. } => {
                let mut pages = Vec::new();
                let mut offset = 0;
                loop {
                    let page = queries::list_pages_page(
                        conn,
                        Some(project.id),
                        None,
                        None,
                        None,
                        Some("sort_order"),
                        Some("asc"),
                        Some(500),
                        Some(offset),
                    )?;
                    pages.extend(page.items);
                    if !page.has_more {
                        break;
                    }
                    offset += 500;
                }
                Body::Pages {
                    pages,
                    folders: q::public_folders(conn, project)?,
                }
            }
            Route::IssueDetail { identifier, .. } => {
                let issue = q::public_issue_by_identifier(conn, project, identifier)?
                    .ok_or_else(not_found)?;
                let comments = all_comments(conn, CommentParent::Issue(issue.id))?;
                let attachments =
                    q::public_entity_attachments(conn, project, AttachmentEntity::Issue, issue.id)?
                        .ok_or_else(not_found)?;
                Body::Issue {
                    issue,
                    comments,
                    attachments,
                    modules: q::public_modules(conn, project)?,
                    labels: q::public_labels(conn, project)?,
                }
            }
            Route::PageDetail { page_id, .. } => {
                let page = q::public_page(conn, project, *page_id)?.ok_or_else(not_found)?;
                let comments = all_comments(conn, CommentParent::Page(page.id))?;
                let attachments =
                    q::public_entity_attachments(conn, project, AttachmentEntity::Page, page.id)?
                        .ok_or_else(not_found)?;
                Body::Page {
                    page,
                    comments,
                    attachments,
                    folders: q::public_folders(conn, project)?,
                }
            }
            Route::Redirect(_) => unreachable!("redirects returned before the public read"),
        };

        Ok(Snapshot {
            project: project.clone(),
            route: route.clone(),
            body,
        })
    })
}

fn issue_collection(
    conn: &rusqlite::Connection,
    project: &Project,
) -> Result<IssueCollection, LificError> {
    let mut issues = Vec::new();
    let mut offset = 0;
    loop {
        let page = queries::list_issues_page(
            conn,
            &ListIssuesQuery {
                project_id: Some(project.id),
                limit: Some(500),
                offset: Some(offset),
                order_by: Some("sequence".into()),
                order: Some("asc".into()),
                ..Default::default()
            },
        )?;
        issues.extend(page.items);
        if !page.has_more {
            break;
        }
        offset += 500;
    }
    populate_relations(conn, project.id, &mut issues)?;
    for issue in &mut issues {
        q::scrub_issue(project, issue);
        issue.description = queries::changes::preview_of(&issue.description);
    }

    Ok(IssueCollection {
        project: project.clone(),
        modules: q::public_modules(conn, project)?,
        labels: q::public_labels(conn, project)?,
        issues,
        assignments: Default::default(),
        current_user_id: 0,
    })
}

/// Add in-project collection relations with one query inside the published
/// read. The canonical relation query excludes cross-project edges, so no
/// foreign issue identifiers enter the public collection.
fn populate_relations(
    conn: &rusqlite::Connection,
    project_id: i64,
    issues: &mut [Issue],
) -> Result<(), LificError> {
    let indexes = issues
        .iter()
        .enumerate()
        .map(|(index, issue)| (issue.id, index))
        .collect::<HashMap<_, _>>();
    let relations = queries::list_project_relations(conn, project_id)?;
    for relation in relations {
        let source = indexes.get(&relation.source_id).copied();
        let target = indexes.get(&relation.target_id).copied();
        match relation.relation_type.as_str() {
            "blocks" => {
                if let Some(index) = source {
                    issues[index]
                        .blocks
                        .push(relation.target_identifier.clone());
                }
                if let Some(index) = target {
                    issues[index]
                        .blocked_by
                        .push(relation.source_identifier.clone());
                }
            }
            "relates_to" => {
                if let Some(index) = source {
                    issues[index]
                        .relates_to
                        .push(relation.target_identifier.clone());
                }
                if let Some(index) = target {
                    issues[index]
                        .relates_to
                        .push(relation.source_identifier.clone());
                }
            }
            "duplicate" => {
                if let Some(index) = source {
                    issues[index]
                        .duplicates
                        .push(relation.target_identifier.clone());
                }
                if let Some(index) = target {
                    issues[index]
                        .duplicated_by
                        .push(relation.source_identifier.clone());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn all_comments(
    conn: &rusqlite::Connection,
    parent: CommentParent,
) -> Result<Vec<Comment>, LificError> {
    let mut comments = Vec::new();
    let mut offset = 0;
    loop {
        let page = q::public_comments(conn, parent, None, Some(500), Some(offset), None)?;
        comments.extend(page.items);
        if !page.has_more {
            break;
        }
        offset = page.next_offset;
    }
    Ok(comments)
}

fn not_found() -> LificError {
    LificError::NotFound("not found".into())
}

#[cfg(test)]
#[path = "data_tests.rs"]
mod tests;
