//! Authorized project structure mutations shared by REST and native views.

use crate::{
    authz,
    db::{DbPool, queries::ResourceTable},
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

/// Authorize and create on one writer transaction, publishing only after commit.
pub(crate) fn commit_create<T>(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
    create: impl FnOnce(&rusqlite::Connection) -> Result<T, LificError>,
) -> Result<T, LificError> {
    authz::require_structure_role(db, identity, project_id)?;
    let created = db.transaction(|conn| {
        let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
        authz::require_structure_role_conn(conn, &identity, project_id)?;
        create(conn)
    })?;
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(created)
}

/// Resolve and authorize the current owner in the same transaction as deletion.
/// The callback receives the owning project for additional rendered-scope checks.
pub(crate) fn commit_delete(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    table: ResourceTable,
    id: i64,
    delete: impl FnOnce(&rusqlite::Connection, i64) -> Result<(), LificError>,
) -> Result<(), LificError> {
    let project_id = db.transaction(|conn| {
        let project_id = crate::db::queries::get_resource_project_id(conn, table, id)?;
        let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
        authz::require_structure_role_conn(conn, &identity, project_id)?;
        delete(conn, project_id)?;
        Ok(project_id)
    })?;
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        api::test_helpers::setup_membership_test,
        db::{
            models::{CreateFolder, CreateLabel, CreatePage, CreateProject, Role},
            queries,
        },
    };

    #[test]
    fn structure_creation_publishes_once_after_committing_folders_and_labels() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let folder = commit_create(&db, &realtime, &identity, project_id, |conn| {
            queries::create_folder(
                conn,
                &CreateFolder {
                    project_id,
                    parent_id: None,
                    name: "Docs".into(),
                },
            )
        })
        .unwrap();
        assert_eq!(
            queries::list_folders(&db.read().unwrap(), project_id).unwrap()[0].id,
            folder.id
        );
        assert_eq!(
            events.try_recv().unwrap().event,
            RealtimeEvent::ProjectUpdated { project_id }
        );
        assert!(events.try_recv().is_err());

        let label = commit_create(&db, &realtime, &identity, project_id, |conn| {
            queries::create_label(
                conn,
                &CreateLabel {
                    project_id,
                    name: "Work".into(),
                    color: "#64748b".into(),
                },
            )
        })
        .unwrap();
        assert_eq!(
            queries::list_labels(&db.read().unwrap(), project_id).unwrap()[0].id,
            label.id
        );
        assert_eq!(
            events.try_recv().unwrap().event,
            RealtimeEvent::ProjectUpdated { project_id }
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_creation_rechecks_revoked_membership_before_running_the_write() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        queries::members::upsert_member(
            &db.write().unwrap(),
            project_id,
            maintainer.id,
            Role::Viewer,
        )
        .unwrap();
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result: Result<(), LificError> =
            commit_create(&db, &realtime, &identity, project_id, |_conn| {
                panic!("revoked authority must be rejected before invoking the write")
            });
        assert!(matches!(result, Err(LificError::Forbidden(_))));
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_creation_rejects_a_foreign_parent_without_inserting_or_publishing() {
        let (db, admin, _, _, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&admin, Transport::Web));
        let parent = {
            let conn = db.write().unwrap();
            let foreign = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "OTHER".into(),
                    name: "Other".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::create_folder(
                &conn,
                &CreateFolder {
                    project_id: foreign.id,
                    parent_id: None,
                    name: "Private".into(),
                },
            )
            .unwrap()
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_create(&db, &realtime, &identity, project_id, |conn| {
            queries::create_folder(
                conn,
                &CreateFolder {
                    project_id,
                    parent_id: Some(parent.id),
                    name: "Rejected".into(),
                },
            )
        });
        assert!(matches!(result, Err(LificError::BadRequest(_))));
        assert!(
            queries::list_folders(&db.read().unwrap(), project_id)
                .unwrap()
                .is_empty()
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_deletion_cascades_nested_folders_and_publishes_once() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        let (root_id, sibling_id, pages, unaffected) = {
            let conn = db.write().unwrap();
            let mut root = None;
            let mut parent_id = None;
            let mut pages = Vec::new();
            for name in ["Root", "Child", "Grandchild"] {
                let folder = queries::create_folder(
                    &conn,
                    &CreateFolder {
                        project_id,
                        parent_id,
                        name: name.into(),
                    },
                )
                .unwrap();
                root.get_or_insert(folder.id);
                parent_id = Some(folder.id);
                pages.push(
                    queries::create_page(
                        &conn,
                        &CreatePage {
                            project_id: Some(project_id),
                            folder_id: Some(folder.id),
                            title: format!("{name} page"),
                            content: "Preserve this body".into(),
                            status: "active".into(),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
            }
            let sibling = queries::create_folder(
                &conn,
                &CreateFolder {
                    project_id,
                    parent_id: None,
                    name: "Keep".into(),
                },
            )
            .unwrap();
            let unaffected = queries::create_page(
                &conn,
                &CreatePage {
                    project_id: Some(project_id),
                    folder_id: Some(sibling.id),
                    title: "Keep this page".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            (root.unwrap(), sibling.id, pages, unaffected)
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        commit_delete(
            &db,
            &realtime,
            &identity,
            ResourceTable::Folders,
            root_id,
            |conn, owner| {
                assert_eq!(owner, project_id);
                queries::delete_folder(conn, root_id)
            },
        )
        .unwrap();

        let conn = db.read().unwrap();
        let remaining = queries::list_folders(&conn, project_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, sibling_id);
        for before in pages {
            let after = queries::get_page(&conn, before.id).unwrap();
            assert_eq!(after.folder_id, None);
            assert_eq!(after.title, before.title);
            assert_eq!(after.content, before.content);
            assert_eq!(after.status, before.status);
            assert!(after.seq > before.seq);
        }
        let preserved = queries::get_page(&conn, unaffected.id).unwrap();
        assert_eq!(preserved.folder_id, unaffected.folder_id);
        assert_eq!(preserved.seq, unaffected.seq);
        assert_eq!(
            events.try_recv().unwrap().event,
            RealtimeEvent::ProjectUpdated { project_id }
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_deletion_rechecks_revoked_membership_before_the_callback() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        let folder = {
            let conn = db.write().unwrap();
            let folder = queries::create_folder(
                &conn,
                &CreateFolder {
                    project_id,
                    parent_id: None,
                    name: "Keep".into(),
                },
            )
            .unwrap();
            queries::members::upsert_member(&conn, project_id, maintainer.id, Role::Viewer)
                .unwrap();
            folder
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_delete(
            &db,
            &realtime,
            &identity,
            ResourceTable::Folders,
            folder.id,
            |_, _| panic!("revoked authority must not reach the deletion callback"),
        );
        assert!(matches!(result, Err(LificError::Forbidden(_))));
        assert_eq!(
            queries::list_folders(&db.read().unwrap(), project_id)
                .unwrap()
                .len(),
            1
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_deletion_missing_row_never_invokes_or_publishes() {
        let (db, _, _, maintainer, _, _, _) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_delete(
            &db,
            &realtime,
            &identity,
            ResourceTable::Folders,
            i64::MAX,
            |_, _| panic!("missing resource must not reach the deletion callback"),
        );
        assert!(matches!(result, Err(LificError::NotFound(_))));
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn structure_deletion_rolls_back_query_failure_without_publication() {
        let (db, _, _, maintainer, _, _, project_id) = setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
        let folder = {
            let conn = db.write().unwrap();
            let folder = queries::create_folder(
                &conn,
                &CreateFolder {
                    project_id,
                    parent_id: None,
                    name: "Keep".into(),
                },
            )
            .unwrap();
            conn.execute_batch(
                "CREATE TRIGGER deny_folder_delete BEFORE DELETE ON folders
                 BEGIN SELECT RAISE(ABORT, 'keep folders'); END;",
            )
            .unwrap();
            folder
        };
        let realtime = RealtimeHub::new();
        let mut events = realtime.subscribe();
        let result = commit_delete(
            &db,
            &realtime,
            &identity,
            ResourceTable::Folders,
            folder.id,
            |conn, owner| {
                assert_eq!(owner, project_id);
                queries::delete_folder(conn, folder.id)
            },
        );
        assert!(result.is_err());
        assert_eq!(
            queries::list_folders(&db.read().unwrap(), project_id).unwrap()[0].id,
            folder.id
        );
        assert!(events.try_recv().is_err());
    }
}
