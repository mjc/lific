//! Authorized project structure creation shared by REST and native views.

use crate::{
    authz,
    db::DbPool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        api::test_helpers::setup_membership_test,
        db::{
            models::{CreateFolder, CreateLabel, CreateProject, Role},
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
}
