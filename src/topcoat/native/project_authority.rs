//! The permission snapshot used by both page controls and cached navigation.

use crate::{
    authz,
    db::{DbPool, models::Role, queries},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub(crate) project_id: i64,
    role: Option<Role>,
    enforced: bool,
    primary_lead: Option<i64>,
    pub(crate) can_edit_content: bool,
    pub(crate) can_edit_structure: bool,
}

impl Snapshot {
    pub(crate) fn encoded(&self) -> String {
        serde_json::to_string(self).expect("project authority contains only serializable scalars")
    }
}

pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Snapshot, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let snapshot = load_conn(&tx, identity, project_id)?;
    tx.commit()?;
    Ok(snapshot)
}

pub(crate) fn load_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Snapshot, LificError> {
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &identity, project_id, Role::Viewer)?;
    let project = queries::get_project(conn, project_id)?;
    let user = identity.as_ref().map(|identity| identity.user.clone());
    let effective = authz::effective_user(conn, &user);
    let role = effective
        .map(|user| queries::members::get_member_role(conn, project_id, user.id))
        .transpose()?
        .flatten();
    Ok(Snapshot {
        project_id,
        role,
        enforced: authz::authz_enforced_conn(conn)?,
        primary_lead: project.lead_user_id,
        can_edit_content: capability(authz::require_role_conn(
            conn,
            &identity,
            project_id,
            Role::Maintainer,
        ))?,
        can_edit_structure: capability(authz::require_structure_role_conn(
            conn, &identity, project_id,
        ))?,
    })
}

fn capability(result: Result<(), LificError>) -> Result<bool, LificError> {
    match result {
        Ok(()) => Ok(true),
        Err(LificError::Forbidden(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_authority_rechecks_previously_resolved_admin_identity() {
        let (db, admin, _, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(
            &admin,
            crate::actor::Transport::Web,
        ));
        assert!(load(&db, &identity, project).unwrap().can_edit_structure);
        {
            let conn = db.write().unwrap();
            conn.execute("UPDATE users SET is_admin = 0 WHERE id = ?1", [admin.id])
                .unwrap();
            queries::members::upsert_member(&conn, project, admin.id, Role::Viewer).unwrap();
        }
        let fresh = load(&db, &identity, project).unwrap();
        assert!(!fresh.can_edit_content);
        assert!(!fresh.can_edit_structure);
    }

    #[test]
    fn project_authority_tracks_role_changes_without_denying_readonly_viewers() {
        let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(
            &viewer,
            crate::actor::Transport::Web,
        ));
        let readonly = load(&db, &identity, project).unwrap();
        assert!(!readonly.can_edit_content);
        assert!(!readonly.can_edit_structure);
        queries::members::upsert_member(&db.write().unwrap(), project, viewer.id, Role::Maintainer)
            .unwrap();
        let editable = load(&db, &identity, project).unwrap();
        assert!(editable.can_edit_content);
        assert!(editable.can_edit_structure);
        assert_ne!(readonly, editable);
        assert_eq!(
            serde_json::from_str::<Snapshot>(&editable.encoded()).unwrap(),
            editable,
        );
    }

    #[test]
    fn project_authority_tracks_enforcement_and_legacy_lead_changes() {
        let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(
            &viewer,
            crate::actor::Transport::Web,
        ));
        let enforced = load(&db, &identity, project).unwrap();
        db.write()
            .unwrap()
            .execute("UPDATE instance_settings SET authz_enforced = 0", [])
            .unwrap();
        let legacy = load(&db, &identity, project).unwrap();
        assert!(legacy.can_edit_content);
        assert_ne!(enforced, legacy);
        db.write()
            .unwrap()
            .execute(
                "UPDATE projects SET lead_user_id = ?1 WHERE id = ?2",
                [viewer.id, project],
            )
            .unwrap();
        let lead = load(&db, &identity, project).unwrap();
        assert!(lead.can_edit_structure);
        assert_ne!(legacy, lead);
    }
}
