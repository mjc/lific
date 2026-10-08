//! Shared instance-member mutations for the API and native administration UI.

use crate::{
    db::{DbPool, models::User},
    error::LificError,
    realtime::RealtimeHub,
    resolve_caller::ResolvedIdentity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Promote,
    Demote,
    Deactivate,
    Reactivate,
}

impl Action {
    fn requires_recent_session(self) -> bool {
        matches!(self, Self::Promote | Self::Reactivate)
    }
}

/// Apply one roster change using the same authorization and database guards
/// regardless of whether it came from the REST API or native Topcoat.
pub(crate) fn mutate(
    db: &DbPool,
    realtime: Option<&RealtimeHub>,
    identity: &Option<ResolvedIdentity>,
    target_id: i64,
    action: Action,
    session_token: Option<&str>,
) -> Result<User, LificError> {
    let caller = crate::api::require_user(identity)?;
    if !caller.is_admin {
        return Err(LificError::Forbidden("only an admin can do this".into()));
    }
    let realtime =
        if action == Action::Deactivate {
            Some(realtime.ok_or_else(|| {
                LificError::Internal("deactivation requires the realtime hub".into())
            })?)
        } else {
            None
        };

    let (user, revoked_ids) = db.transaction(|tx| {
        let fresh_caller = if action.requires_recent_session() {
            let token = session_token.ok_or_else(|| {
                LificError::Forbidden(crate::auth::RECENT_AUTH_REQUIRED_MESSAGE.into())
            })?;
            crate::auth::revalidate_recent_session(tx, token, caller.id)?
        } else {
            crate::auth::fresh_caller(tx, caller.id)?
        };
        crate::auth::require_fresh_admin(&fresh_caller)?;

        match action {
            Action::Promote => Ok((
                crate::db::queries::users::set_admin_guarded(tx, target_id, true)?,
                Vec::new(),
            )),
            Action::Demote => Ok((
                crate::db::queries::users::set_admin_guarded(tx, target_id, false)?,
                Vec::new(),
            )),
            Action::Deactivate => {
                let user = crate::db::queries::users::set_active(tx, target_id, false)?;
                let owned_bots = crate::db::queries::users::owned_bot_ids(tx, user.id)?;
                Ok((user, owned_bots))
            }
            Action::Reactivate => Ok((
                crate::db::queries::users::set_active(tx, target_id, true)?,
                Vec::new(),
            )),
        }
    })?;

    if action == Action::Deactivate {
        let realtime = realtime.expect("deactivation validated the realtime hub before writing");
        realtime.revoke_user(user.id);
        for id in revoked_ids {
            realtime.revoke_user(id);
        }
    }

    Ok(user)
}
