use topcoat::{context::CxTestBuilder, router::request::Request};

use super::*;
use crate::{
    auth::AuthState,
    db::{
        self,
        models::{
            AttachmentActor, AttachmentEntity, CommentActor, CreateFolder, CreateIssue, CreatePage,
            CreateProject, CreateWait,
        },
        queries::{
            self,
            comments::{CommentCursor, CommentParent},
        },
    },
    error::LificError,
};

fn fixture() -> (
    db::DbPool,
    Cx,
    crate::db::models::Project,
    crate::db::models::Project,
) {
    let db = db::open_memory().unwrap();
    let conn = db.write().unwrap();
    let lead = queries::users::create_user(
        &conn,
        &crate::db::models::CreateUser {
            username: "lead".into(),
            email: "lead@test.local".into(),
            password: "testpassword1".into(),
            display_name: None,
            is_admin: false,
            is_bot: false,
        },
    )
    .unwrap();
    let public = queries::create_project(
        &conn,
        &CreateProject {
            name: "Published".into(),
            identifier: "PUB".into(),
            lead_user_id: Some(lead.id),
            ..Default::default()
        },
    )
    .unwrap();
    conn.execute(
        "UPDATE projects SET is_public = 1 WHERE id = ?1",
        [public.id],
    )
    .unwrap();
    let private = queries::create_project(
        &conn,
        &CreateProject {
            name: "Private".into(),
            identifier: "PRIV".into(),
            ..Default::default()
        },
    )
    .unwrap();
    drop(conn);

    let (parts, ()) = Request::new(()).into_parts();
    let cx = CxTestBuilder::new()
        .app_context(AuthState {
            db: db.clone(),
            public_url: "https://test.local".into(),
            required: true,
        })
        .request_context(parts)
        .build();
    (db, cx, public, private)
}

fn route_issues(project: &str) -> Route {
    Route::Issues {
        project: project.into(),
    }
}

fn add_comment(
    conn: &rusqlite::Connection,
    project: &crate::db::models::Project,
    parent: CommentParent,
    content: &str,
) -> crate::db::models::Comment {
    queries::comments::create_comment_with_mentions(
        conn,
        parent,
        Some(project.id),
        CommentActor {
            user_id: project.lead_user_id.unwrap(),
            is_admin: false,
        },
        AttachmentActor::Unattributed,
        content,
        false,
    )
    .unwrap()
}

#[test]
fn issue_collection_reads_every_page_and_scrubs_private_issue_fields() {
    let (db, cx, public, private) = fixture();
    let mut first = None;
    {
        let conn = db.write().unwrap();
        for index in 0..501 {
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: public.id,
                    title: format!("Public {index}"),
                    source: Some(format!("private-import:{index}")),
                    ..Default::default()
                },
            )
            .unwrap();
            if index == 0 {
                first = Some(issue);
            }
        }
        queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: private.id,
                title: "Private issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let snapshot = load(&cx, &route_issues("PUB")).unwrap();
    let Body::Issues(collection) = snapshot.body else {
        panic!("issues route must return an issue collection");
    };
    assert_eq!(snapshot.project.lead_user_id, None);
    assert_eq!(collection.issues.len(), 501);
    assert!(collection.assignments.is_empty());
    assert_eq!(collection.current_user_id, 0);
    assert!(collection.issues.iter().all(|issue| issue.source.is_none()));
    assert!(
        collection
            .issues
            .iter()
            .all(|issue| issue.project_id == public.id)
    );
    assert_eq!(collection.issues[0].id, first.unwrap().id);

    let board = load(
        &cx,
        &Route::Board {
            project: "PUB".into(),
        },
    )
    .unwrap();
    let Body::Issues(board) = board.body else {
        panic!("board route must reuse the issue collection");
    };
    assert_eq!(board.issues.len(), 501);
}

#[test]
fn unpublished_projects_and_foreign_or_deleted_details_are_not_found() {
    let (db, cx, public, private) = fixture();
    let (public_issue, foreign_issue, page, foreign_page) = {
        let conn = db.write().unwrap();
        let public_issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Live".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign_issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: private.id,
                title: "Foreign".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(public.id),
                title: "Published page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign_page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(private.id),
                title: "Private page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (public_issue, foreign_issue, page, foreign_page)
    };

    let detail = |identifier: String| Route::IssueDetail {
        project: "PUB".into(),
        identifier,
    };
    assert!(load(&cx, &detail(public_issue.identifier.clone())).is_ok());
    assert!(matches!(
        load(&cx, &detail(foreign_issue.identifier)),
        Err(LificError::NotFound(_))
    ));
    assert!(matches!(
        load(
            &cx,
            &Route::PageDetail {
                project: "PUB".into(),
                page_id: foreign_page.id,
            }
        ),
        Err(LificError::NotFound(_))
    ));

    db.write()
        .unwrap()
        .execute(
            "UPDATE issues SET deleted_at = datetime('now') WHERE id = ?1",
            [public_issue.id],
        )
        .unwrap();
    db.write()
        .unwrap()
        .execute(
            "UPDATE pages SET deleted_at = datetime('now') WHERE id = ?1",
            [page.id],
        )
        .unwrap();
    assert!(matches!(
        load(&cx, &detail(public_issue.identifier)),
        Err(LificError::NotFound(_))
    ));
    assert!(matches!(
        load(
            &cx,
            &Route::PageDetail {
                project: "PUB".into(),
                page_id: page.id,
            }
        ),
        Err(LificError::NotFound(_))
    ));

    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 0 WHERE id = ?1",
            [public.id],
        )
        .unwrap();
    assert!(matches!(
        load(&cx, &route_issues("PUB")),
        Err(LificError::NotFound(_))
    ));
    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [public.id],
        )
        .unwrap();
    assert!(load(&cx, &route_issues("PUB")).is_ok());
    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 0 WHERE id = ?1",
            [public.id],
        )
        .unwrap();
    assert!(matches!(
        load(&cx, &route_issues("PUB")),
        Err(LificError::NotFound(_))
    ));
}

#[test]
fn page_collection_is_complete_but_page_comments_start_with_latest_bounded_page() {
    let (db, cx, public, _) = fixture();
    let (parent, comment_ids) = {
        let conn = db.write().unwrap();
        let mut parent = None;
        let mut comment_ids = Vec::new();
        let author = public.lead_user_id.unwrap();
        for index in 0..501 {
            let page = queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(public.id),
                    title: format!("Published page {index}"),
                    status: "active".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            if index == 0 {
                parent = Some(page);
            }
        }
        let parent = parent.unwrap();
        for _ in 0..501 {
            let comment = queries::comments::create_comment_with_mentions(
                &conn,
                CommentParent::Page(parent.id),
                Some(public.id),
                CommentActor {
                    user_id: author,
                    is_admin: false,
                },
                AttachmentActor::Unattributed,
                "public page note",
                false,
            )
            .unwrap();
            comment_ids.push(comment.id);
        }
        conn.execute(
            "UPDATE comments SET created_at = '2026-01-01 00:00:00' WHERE page_id = ?1",
            [parent.id],
        )
        .unwrap();
        (parent, comment_ids)
    };

    let snapshot = load(
        &cx,
        &Route::Pages {
            project: "PUB".into(),
        },
    )
    .unwrap();
    let Body::Pages { pages, .. } = snapshot.body else {
        panic!("pages route must return the page collection");
    };
    assert_eq!(pages.len(), 501);
    assert!(pages.iter().all(|page| page.project_id == Some(public.id)));

    let snapshot = load(
        &cx,
        &Route::PageDetail {
            project: "PUB".into(),
            page_id: parent.id,
        },
    )
    .unwrap();
    let Body::Page { comments, .. } = snapshot.body else {
        panic!("page detail route must return the page body");
    };
    assert_eq!(comments.items.len(), 50);
    assert!(comments.has_more);
    assert_eq!(comments.next_offset, 50);
    assert!(
        comments
            .items
            .windows(2)
            .all(|pair| pair[0].id > pair[1].id)
    );
    assert_eq!(
        comments
            .items
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>(),
        comment_ids
            .iter()
            .rev()
            .take(50)
            .copied()
            .collect::<Vec<_>>()
    );
}

#[test]
fn issue_detail_starts_with_the_latest_fifty_comments() {
    let (db, cx, public, _) = fixture();
    let (issue, ids) = {
        let conn = db.write().unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let ids = (0..55)
            .map(|index| {
                add_comment(
                    &conn,
                    &public,
                    CommentParent::Issue(issue.id),
                    &format!("comment {index}"),
                )
                .id
            })
            .collect::<Vec<_>>();
        conn.execute(
            "UPDATE comments SET created_at = '2026-01-01 00:00:00' WHERE issue_id = ?1",
            [issue.id],
        )
        .unwrap();
        (issue, ids)
    };

    let snapshot = load(
        &cx,
        &Route::IssueDetail {
            project: "PUB".into(),
            identifier: issue.identifier,
        },
    )
    .unwrap();
    let Body::Issue { comments, .. } = snapshot.body else {
        panic!("issue detail route must return the issue body");
    };
    assert_eq!(comments.items.len(), 50);
    assert!(comments.has_more);
    assert_eq!(
        comments
            .items
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>(),
        ids.iter().rev().take(50).copied().collect::<Vec<_>>()
    );
}

#[test]
fn comment_keyset_page_survives_equal_time_inserts_and_deletes() {
    let (db, cx, public, _) = fixture();
    let (issue, ids) = {
        let conn = db.write().unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let ids = (0..53)
            .map(|index| {
                add_comment(
                    &conn,
                    &public,
                    CommentParent::Issue(issue.id),
                    &format!("comment {index}"),
                )
                .id
            })
            .collect::<Vec<_>>();
        conn.execute(
            "UPDATE comments SET created_at = '2026-01-01 00:00:00' WHERE issue_id = ?1",
            [issue.id],
        )
        .unwrap();
        (issue, ids)
    };

    let first = read_comments(&cx, "PUB", CommentParent::Issue(issue.id), None).unwrap();
    assert_eq!(first.items.len(), 50);
    assert!(first.has_more);
    let cursor = CommentCursor::before(first.items.last().unwrap());

    {
        let conn = db.write().unwrap();
        add_comment(
            &conn,
            &public,
            CommentParent::Issue(issue.id),
            "newer than the captured cursor",
        );
        conn.execute(
            "UPDATE comments SET deleted_at = datetime('now') WHERE id = ?1",
            [ids[52]],
        )
        .unwrap();
    }

    let next = read_comments(&cx, "PUB", CommentParent::Issue(issue.id), Some(&cursor)).unwrap();
    assert_eq!(
        next.items
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>(),
        ids[..3].iter().rev().copied().collect::<Vec<_>>()
    );
    assert!(!next.has_more);
}

#[test]
fn comment_byte_limited_page_keeps_a_usable_cursor() {
    let (db, cx, public, _) = fixture();
    let (issue, expected_count) = {
        let conn = db.write().unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..40 {
            add_comment(
                &conn,
                &public,
                CommentParent::Issue(issue.id),
                &"x".repeat(60 * 1024),
            );
        }
        (issue, 40)
    };

    let mut page = read_comments(&cx, "PUB", CommentParent::Issue(issue.id), None).unwrap();
    assert!(page.items.len() < expected_count);
    assert!(page.has_more);
    assert!(page.budget_limited);
    let mut ids = Vec::new();
    loop {
        assert!(!page.items.is_empty());
        assert!(page.items.len() <= 50);
        ids.extend(page.items.iter().map(|comment| comment.id));
        if !page.has_more {
            break;
        }
        let cursor = CommentCursor::before(page.items.last().unwrap());
        page = read_comments(&cx, "PUB", CommentParent::Issue(issue.id), Some(&cursor)).unwrap();
    }
    assert_eq!(ids.len(), expected_count);
    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        expected_count
    );
}

#[test]
fn comment_loader_rejects_foreign_deleted_and_unpublished_parents() {
    let (db, cx, public, private) = fixture();
    let (issue, foreign_issue, page, foreign_page) = {
        let conn = db.write().unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign_issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: private.id,
                title: "Private issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(public.id),
                title: "Published page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign_page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(private.id),
                title: "Private page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (issue, foreign_issue, page, foreign_page)
    };

    assert!(read_comments(&cx, "PUB", CommentParent::Issue(issue.id), None).is_ok());
    for parent in [
        CommentParent::Issue(foreign_issue.id),
        CommentParent::Page(foreign_page.id),
    ] {
        assert!(matches!(
            read_comments(&cx, "PUB", parent, None),
            Err(LificError::NotFound(_))
        ));
    }
    assert!(matches!(
        read_comments(&cx, "PRIV", CommentParent::Page(foreign_page.id), None),
        Err(LificError::NotFound(_))
    ));

    db.write()
        .unwrap()
        .execute(
            "UPDATE pages SET deleted_at = datetime('now') WHERE id = ?1",
            [page.id],
        )
        .unwrap();
    assert!(matches!(
        read_comments(&cx, "PUB", CommentParent::Page(page.id), None),
        Err(LificError::NotFound(_))
    ));
    db.write()
        .unwrap()
        .execute(
            "UPDATE issues SET deleted_at = datetime('now') WHERE id = ?1",
            [issue.id],
        )
        .unwrap();
    assert!(matches!(
        read_comments(&cx, "PUB", CommentParent::Issue(issue.id), None),
        Err(LificError::NotFound(_))
    ));
    db.write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 0 WHERE id = ?1",
            [public.id],
        )
        .unwrap();
    assert!(matches!(
        read_comments(&cx, "PUB", CommentParent::Issue(issue.id), None),
        Err(LificError::NotFound(_))
    ));
}

#[test]
fn page_detail_includes_only_the_published_projects_folder_catalog() {
    let (db, cx, public, private) = fixture();
    let (page, public_folder) = {
        let conn = db.write().unwrap();
        let public_folder = queries::create_folder(
            &conn,
            &CreateFolder {
                project_id: public.id,
                parent_id: None,
                name: "Public folder".into(),
            },
        )
        .unwrap();
        queries::create_folder(
            &conn,
            &CreateFolder {
                project_id: private.id,
                parent_id: None,
                name: "Private folder".into(),
            },
        )
        .unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(public.id),
                folder_id: Some(public_folder.id),
                title: "Published page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (page, public_folder)
    };

    let snapshot = load(
        &cx,
        &Route::PageDetail {
            project: "PUB".into(),
            page_id: page.id,
        },
    )
    .unwrap();
    let Body::Page { folders, .. } = snapshot.body else {
        panic!("page detail route must return folder metadata");
    };
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, public_folder.id);
}

#[test]
fn collection_scrubs_private_wait_notes_and_cross_project_relation_identifiers() {
    let (db, cx, public, private) = fixture();
    let (issue, sibling) = {
        let conn = db.write().unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published issue".into(),
                source: Some("private source repository/path#99".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let sibling = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Published sibling".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: private.id,
                title: "Private blocker".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::link_issues(&conn, issue.id, sibling.id, "blocks").unwrap();
        queries::link_issues(&conn, issue.id, foreign.id, "blocks").unwrap();
        queries::link_issues(&conn, foreign.id, issue.id, "blocks").unwrap();
        queries::link_issues(&conn, foreign.id, issue.id, "relates_to").unwrap();
        queries::waits::add_wait(
            &conn,
            issue.id,
            &CreateWait {
                user: Some("lead".into()),
                note: Some("private note mentioning hidden project details".into()),
                ..Default::default()
            },
            public.lead_user_id,
        )
        .unwrap();
        (issue, sibling)
    };

    let snapshot = load(&cx, &route_issues("PUB")).unwrap();
    let Body::Issues(collection) = snapshot.body else {
        panic!("issues route must return an issue collection");
    };
    let issue = collection
        .issues
        .iter()
        .find(|candidate| candidate.id == issue.id)
        .unwrap();
    assert_eq!(issue.source, None);
    assert!(issue.waits.is_empty());
    assert_eq!(issue.blocks, vec![sibling.identifier]);
    assert!(issue.blocked_by.is_empty());
    assert!(issue.relates_to.is_empty());
    let public_json = serde_json::to_string(issue).unwrap();
    assert!(!public_json.contains("private note mentioning"));
    assert!(!public_json.contains("private source repository"));
    assert!(!public_json.contains("\"lead\""));
    assert!(!public_json.contains("PRIV-1"));
}

#[test]
fn issue_detail_keeps_comment_attribution_but_scrubs_accounts_and_attachment_uploaders() {
    let (db, cx, public, _) = fixture();
    let (issue, comment_id, attachment_id) = {
        let conn = db.write().unwrap();
        let user = queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "author".into(),
                email: "author@test.local".into(),
                password: "testpassword1".into(),
                display_name: Some("Public Author".into()),
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: public.id,
                title: "Public issue".into(),
                source: Some("private import data".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let comment = queries::comments::create_comment_with_mentions(
            &conn,
            CommentParent::Issue(issue.id),
            Some(public.id),
            CommentActor {
                user_id: user.id,
                is_admin: false,
            },
            AttachmentActor::TrustedLocal,
            "comment body",
            false,
        )
        .unwrap();
        let attachment = queries::attachments::create_attachment(
            &conn,
            &"a".repeat(64),
            "public.txt",
            "text/plain",
            12,
            Some(user.id),
        )
        .unwrap();
        queries::attachments::link_attachment(
            &conn,
            attachment.id,
            AttachmentEntity::Issue,
            issue.id,
        )
        .unwrap();
        (issue, comment.id, attachment.id)
    };

    let snapshot = load(
        &cx,
        &Route::IssueDetail {
            project: "PUB".into(),
            identifier: issue.identifier,
        },
    )
    .unwrap();
    let Body::Issue {
        issue: public_issue,
        comments,
        attachments,
        ..
    } = snapshot.body
    else {
        panic!("issue detail route must return the complete issue body");
    };
    assert_eq!(public_issue.source, None);
    let comment = comments
        .items
        .iter()
        .find(|comment| comment.id == comment_id)
        .unwrap();
    assert_eq!(comment.author_display_name, "Public Author");
    assert_eq!(comment.user_id, 0);
    assert!(comment.author.is_empty());
    let attachment = attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .unwrap();
    assert_eq!(attachment.uploader_id, None);
}
