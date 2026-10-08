//! Page reads and mutations shared by native Topcoat and REST transports.

use crate::{
    authz,
    db::{
        DbPool,
        models::{
            AttachmentActor, CommentActor, CreatePage, Folder, Label, Page, Role, UpdatePage,
        },
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

/// Page authorization uses project roles for project pages and workspace
/// administrator authority for workspace pages.
pub(crate) fn require_page_role(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: Option<i64>,
    role: Role,
) -> Result<(), LificError> {
    match project_id {
        Some(project_id) => authz::require_role(db, identity, project_id, role),
        None => authz::require_workspace_admin(db, identity),
    }
}

pub(crate) fn require_page_role_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: Option<i64>,
    role: Role,
) -> Result<(), LificError> {
    authz::require_project_or_workspace_role_conn(conn, identity, project_id, role)
}

/// The bounded fields rendered by the Pages tree. Page bodies never escape a
/// list read, keeping native tree state small even for long documents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PageRow {
    pub(crate) id: i64,
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) preview: String,
    pub(crate) status: String,
    pub(crate) folder_id: Option<i64>,
    pub(crate) pinned: bool,
    pub(crate) labels: Vec<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PageStructure {
    pub(crate) folders: Vec<Folder>,
    pub(crate) labels: Vec<Label>,
}

pub(crate) fn project_structure(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<PageStructure, LificError> {
    authz::require_role(db, identity, project_id, Role::Viewer)?;
    let conn = db.read()?;
    Ok(PageStructure {
        folders: crate::db::queries::list_folders(&conn, project_id)?,
        labels: crate::db::queries::list_labels(&conn, project_id)?,
    })
}

/// List every project page, preserving the REST query's default order and
/// walking the database's 500-row pages so large projects are not truncated.
pub(crate) fn list_project_pages(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Vec<PageRow>, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    authz::require_role_conn(&tx, identity, project_id, Role::Viewer)?;
    let mut rows = Vec::new();
    let mut offset = 0_i64;
    loop {
        let page = crate::db::queries::list_pages_page(
            &tx,
            Some(project_id),
            None,
            None,
            None,
            None,
            None,
            Some(500),
            Some(offset),
        )?;
        rows.extend(page.items.into_iter().map(into_row));
        if !page.has_more {
            break;
        }
        offset += 500;
    }
    tx.commit()?;
    Ok(rows)
}

fn into_row(page: Page) -> PageRow {
    let preview = page
        .content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(200)
        .collect();
    PageRow {
        id: page.id,
        identifier: page.identifier,
        title: page.title,
        preview,
        status: page.status,
        folder_id: page.folder_id,
        pinned: page.pinned,
        labels: page.labels,
        created_at: page.created_at,
        updated_at: page.updated_at,
    }
}

/// Read one page only after authorizing its project or workspace scope.
pub(crate) fn get(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<Page, LificError> {
    let page = {
        let conn = db.read()?;
        crate::db::queries::get_page(&conn, id)?
    };
    require_page_role(db, identity, page.project_id, Role::Viewer)?;
    Ok(page)
}

pub(crate) fn resolve(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: &str,
) -> Result<Page, LificError> {
    let page = {
        let conn = db.read()?;
        let id = crate::db::queries::resolve_page_identifier(&conn, identifier)?;
        crate::db::queries::get_page(&conn, id)?
    };
    require_page_role(db, identity, page.project_id, Role::Viewer)?;
    Ok(page)
}

/// Create a page in the caller's scope; linked uploads are attributed to that
/// authenticated user and membership is checked again in the write transaction.
pub(crate) fn commit_create(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    mut input: CreatePage,
) -> Result<Page, LificError> {
    require_page_role(db, identity, input.project_id, Role::Maintainer)?;
    let user = crate::api::require_user(identity)?;
    input.attachments = AttachmentActor::Authenticated(CommentActor::from(&user));
    let page = db.transaction(|conn| {
        require_page_role_conn(conn, identity, input.project_id, Role::Maintainer)?;
        crate::db::queries::create_page(conn, &input)
    })?;
    publish_project_update(realtime, &page);
    Ok(page)
}

/// Save with the database's `expected_seq` conflict boundary and fresh role
/// recheck. The caller supplies its actor scope around this synchronous method.
pub(crate) fn commit_update(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    input: UpdatePage,
) -> Result<Page, LificError> {
    commit_update_with(db, realtime, identity, id, |_| input)
}

pub(crate) enum PageLabelChange<'a> {
    Attach(&'a str),
    Remove(&'a str),
}

/// Apply one label intent to the current set, preserving labels attached by
/// other writers and avoiding a sequence conflict over unrelated page fields.
pub(crate) fn commit_label_change(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    change: PageLabelChange<'_>,
) -> Result<Page, LificError> {
    commit_update_with(db, realtime, identity, id, |current| {
        let mut labels = current.labels.clone();
        match change {
            PageLabelChange::Attach(name) => {
                if !labels.iter().any(|label| label == name) {
                    labels.push(name.to_owned());
                }
            }
            PageLabelChange::Remove(name) => labels.retain(|label| label != name),
        }
        UpdatePage {
            labels: Some(labels),
            ..Default::default()
        }
    })
}

fn commit_update_with(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    patch: impl FnOnce(&Page) -> UpdatePage,
) -> Result<Page, LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::get_page(&conn, id)?.project_id
    };
    require_page_role(db, identity, project_id, Role::Maintainer)?;
    crate::api::require_user(identity)?;
    let page = db.transaction(|conn| {
        let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
        let user = crate::api::require_user(&identity)?;
        let current = crate::db::queries::get_page(conn, id)?;
        require_page_role_conn(conn, &identity, current.project_id, Role::Maintainer)?;
        let mut input = patch(&current);
        input.attachments = AttachmentActor::Authenticated(CommentActor::from(&user));
        crate::db::queries::update_page(conn, id, &input)
    })?;
    publish_project_update(realtime, &page);
    Ok(page)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageDelete {
    pub(crate) page_id: i64,
    pub(crate) project_id: Option<i64>,
    pub(crate) tombstone_seq: i64,
}

/// Soft-delete a page through the authorized writer transaction.
pub(crate) fn commit_delete(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<PageDelete, LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::get_page(&conn, id)?.project_id
    };
    require_page_role(db, identity, project_id, Role::Maintainer)?;
    let deleted = db.transaction(|conn| {
        let project_id = crate::db::queries::get_page(conn, id)?.project_id;
        require_page_role_conn(conn, identity, project_id, Role::Maintainer)?;
        crate::db::queries::delete_page(conn, id)?;
        Ok(PageDelete {
            page_id: id,
            project_id,
            tombstone_seq: crate::db::queries::page_seq(conn, id)?,
        })
    })?;
    if let Some(project_id) = deleted.project_id {
        realtime.send_with_seq(
            RealtimeEvent::ProjectUpdated { project_id },
            deleted.tombstone_seq,
        );
    }
    Ok(deleted)
}

fn publish_project_update(realtime: &RealtimeHub, page: &Page) {
    if let Some(project_id) = page.project_id {
        realtime.send_with_seq(RealtimeEvent::ProjectUpdated { project_id }, page.seq);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        api::test_helpers::setup_membership_test,
        auth,
        db::{models::CreatePage, queries},
    };

    #[test]
    fn project_page_list_returns_created_pages_for_a_viewer() {
        let (db, _, _, _, viewer, _, project_id) = setup_membership_test();
        let expected = {
            let conn = db.write().unwrap();
            queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(project_id),
                    title: "Design notes".into(),
                    content: "# First draft\n\nMore detail".into(),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let identity = Some(auth::fresh_identity(&viewer, Transport::Web));

        let pages = list_project_pages(&db, &identity, project_id).unwrap();

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, expected.id);
        assert_eq!(pages[0].title, "Design notes");
        assert_eq!(pages[0].preview, "# First draft");
    }

    #[test]
    fn project_page_list_reads_past_the_database_page_limit_and_drops_bodies() {
        let (db, _, _, _, viewer, _, project_id) = setup_membership_test();
        {
            let conn = db.write().unwrap();
            for index in 0..501 {
                queries::create_page(
                    &conn,
                    &CreatePage {
                        project_id: Some(project_id),
                        title: format!("Page {index}"),
                        content: format!("preview {index}\n{}", "x".repeat(2048)),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        let identity = Some(auth::fresh_identity(&viewer, Transport::Web));

        let pages = list_project_pages(&db, &identity, project_id).unwrap();

        assert_eq!(pages.len(), 501);
        assert_eq!(pages.last().unwrap().preview, "preview 500");
        assert!(!pages.last().unwrap().preview.contains(&"x".repeat(100)));
    }

    #[test]
    fn page_update_keeps_the_winning_content_when_expected_sequence_is_stale() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(auth::fresh_identity(&maintainer, Transport::Web));
        let original = {
            let conn = db.write().unwrap();
            queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(project_id),
                    title: "Original".into(),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let realtime = RealtimeHub::new();
        commit_update(
            &db,
            &realtime,
            &identity,
            original.id,
            UpdatePage {
                title: Some("Winner".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let conflict = commit_update(
            &db,
            &realtime,
            &identity,
            original.id,
            UpdatePage {
                content: Some("stale write".into()),
                expected_seq: Some(original.seq),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(matches!(conflict, LificError::UpdateConflict { .. }));
        assert_eq!(get(&db, &identity, original.id).unwrap().title, "Winner");
        assert_eq!(get(&db, &identity, original.id).unwrap().content, "");
    }

    #[test]
    fn page_write_rechecks_membership_inside_the_authorized_service() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(auth::fresh_identity(&maintainer, Transport::Web));
        let page = {
            let conn = db.write().unwrap();
            queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(project_id),
                    title: "Before revocation".into(),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project_id, maintainer.id],
            )
            .unwrap();

        let result = commit_update(
            &db,
            &RealtimeHub::new(),
            &identity,
            page.id,
            UpdatePage {
                title: Some("Must not save".into()),
                ..Default::default()
            },
        );

        assert!(matches!(result, Err(LificError::Forbidden(_))));
        let conn = db.read().unwrap();
        assert_eq!(
            queries::get_page(&conn, page.id).unwrap().title,
            "Before revocation"
        );
    }
}
