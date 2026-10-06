//! Project analytics reads share current authorization and one SQLite snapshot.

use crate::{
    authz,
    db::{
        DbPool,
        models::{InsightsPayload, Role},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) fn get(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project: i64,
    weeks: Option<i64>,
) -> Result<InsightsPayload, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    authz::require_role_conn(&tx, &current, project, Role::Viewer)?;
    let payload =
        queries::insights::get_insights(&tx, project, queries::insights::clamp_weeks(weeks))?;
    tx.commit()?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Transport;

    #[test]
    fn insights_shared_read_revalidates_membership_and_identity() {
        let (db, _, _, _, viewer, _, project) = crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        assert_eq!(get(&db, &identity, project, None).unwrap().weeks, 12);
        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project, viewer.id],
            )
            .unwrap();
        assert!(matches!(
            get(&db, &identity, project, Some(4)),
            Err(LificError::Forbidden(_))
        ));
        db.write()
            .unwrap()
            .execute("UPDATE users SET is_active=0 WHERE id=?1", [viewer.id])
            .unwrap();
        assert!(
            matches!(get(&db,&identity,project,None),Err(LificError::Forbidden(message)) if message=="authentication required")
        );
    }
}
