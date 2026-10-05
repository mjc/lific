//! Authorized project catalog shared by REST and native views.

use std::collections::HashSet;

use crate::api::require_user;
use crate::{
    authz::{self, filter_visible},
    db::{DbPool, models::Project},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) fn sidebar_visibility(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Option<HashSet<i64>>, LificError> {
    let fresh = crate::auth::fresh_caller(conn, user_id)?;
    let effective = authz::effective_user(conn, &Some(crate::auth::fresh_auth_user(&fresh)));
    if matches!(&effective, Some(user) if user.is_admin) || !authz::authz_enforced_conn(conn)? {
        return Ok(None);
    }
    let Some(user) = effective else {
        return Ok(Some(HashSet::new()));
    };
    Ok(Some(
        crate::db::queries::members::list_project_ids_for_user(conn, user.id)?
            .into_iter()
            .collect(),
    ))
}

pub(crate) fn normalize_sidebar_ranks(mut projects: Vec<Project>) -> Vec<Project> {
    for (position, project) in projects.iter_mut().enumerate() {
        project.sort_order = position as i64;
    }
    projects
}

pub(crate) fn list_visible_projects(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
) -> Result<Vec<Project>, LificError> {
    // A valid unbound key can precede the first user. There is no preference
    // owner in that case; preserve the existing visibility-filtered listing.
    if identity.is_none() {
        let visible = authz::visible_project_ids(db, identity)?;
        let conn = db.read()?;
        let projects = crate::db::queries::list_projects(&conn)?;
        return Ok(filter_visible(projects, &visible, |p| Some(p.id)));
    }
    let user = require_user(identity)?;
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let visible = sidebar_visibility(&tx, user.id)?;
    let projects = crate::db::queries::list_projects_for_user(&tx, user.id)?;
    let projects = normalize_sidebar_ranks(filter_visible(projects, &visible, |p| Some(p.id)));
    tx.commit()?;
    Ok(projects)
}

/// Create with the same fresh authority, lead grant gate, and event as REST.
/// Transport adapters preserve their actor scope and supply a verified session.
pub(crate) fn create_project(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    session_token: Option<&str>,
    mut input: crate::db::models::CreateProject,
) -> Result<Project, LificError> {
    let caller = crate::api::require_user(identity)?;

    let project = db.transaction(|tx| {
        let fresh = crate::auth::fresh_caller(tx, caller.id)?;
        let effective =
            crate::authz::effective_user(tx, &Some(crate::auth::fresh_auth_user(&fresh)))
                .ok_or_else(|| LificError::Forbidden("authentication required".into()))?;

        match input.lead_user_id {
            // No lead supplied means the creator leads it.
            // Without this, `require_project_lead` rejects everyone but admins
            // and the project is unowned.
            None => input.lead_user_id = Some(effective.id),
            // Naming yourself grants nothing new.
            Some(id) if id == effective.id => {}
            // Naming anybody else does, so it needs a recent human sign-in.
            Some(_) => {
                let token = session_token.ok_or_else(|| {
                    LificError::Forbidden("recent authentication required".into())
                })?;
                let session_user = crate::auth::revalidate_recent_session(tx, token, caller.id)?;
                // A session belongs to a human, so the effective user is that
                // human; assert it rather than assume it.
                if session_user.id != effective.id {
                    return Err(LificError::Forbidden(
                        "recent authentication required".into(),
                    ));
                }
            }
        }

        crate::db::queries::create_project(tx, &input)
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectCreated {
        project_id: project.id,
    });
    Ok(project)
}

#[cfg(test)]
mod creation_contract {
    use super::*;
    use crate::{
        actor::{ActorCtx, Transport},
        db::{models::CreateProject, queries},
    };
    use tokio::sync::broadcast::error::TryRecvError;

    fn setup() -> (DbPool, crate::db::models::User, crate::db::models::User) {
        let db = crate::db::open_memory().unwrap();
        let (creator, other) = {
            let conn = db.write().unwrap();
            let mut users = Vec::new();
            for name in ["creator", "other"] {
                conn.execute("INSERT INTO users (username,email,password_hash,display_name,is_admin,is_bot) VALUES (?1,?2,'x',?1,0,0)", rusqlite::params![name, format!("{name}@example.test")]).unwrap();
                users
                    .push(queries::users::get_user_by_id(&conn, conn.last_insert_rowid()).unwrap());
            }
            (users.remove(0), users.remove(0))
        };
        (db, creator, other)
    }
    fn input(lead: Option<i64>) -> CreateProject {
        CreateProject {
            name: "Created through shared service".into(),
            identifier: "NEW".into(),
            lead_user_id: lead,
            ..Default::default()
        }
    }
    fn counts(db: &DbPool) -> (i64, i64, i64) {
        let conn = db.read().unwrap();
        let count = |table: &str| {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        };
        (
            count("projects"),
            count("project_members"),
            count("audit_log"),
        )
    }
    #[tokio::test]
    async fn creation_default_self_and_recent_other_lead_commit_once_with_web_actor() {
        for kind in ["default", "self", "other"] {
            let (db, creator, other) = setup();
            let identity = Some(crate::auth::fresh_identity(&creator, Transport::Web));
            let token = queries::users::create_session(&db.write().unwrap(), creator.id, None)
                .unwrap()
                .token;
            let hub = crate::realtime::RealtimeHub::new();
            let mut events = hub.subscribe();
            let lead = match kind {
                "default" => None,
                "self" => Some(creator.id),
                _ => Some(other.id),
            };
            let project = crate::actor::scope(
                ActorCtx {
                    user_id: Some(creator.id),
                    transport: Transport::Web,
                },
                async {
                    create_project(
                        &db,
                        &hub,
                        &identity,
                        if kind == "other" {
                            Some(token.as_str())
                        } else {
                            None
                        },
                        input(lead),
                    )
                    .unwrap()
                },
            )
            .await;
            assert_eq!(project.lead_user_id, Some(lead.unwrap_or(creator.id)));
            let conn = db.read().unwrap();
            let members: Vec<(i64, String)> = {
                let mut statement = conn
                    .prepare("SELECT user_id,role FROM project_members WHERE project_id=?1")
                    .unwrap();
                statement
                    .query_map([project.id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap()
            };
            assert_eq!(members, vec![(lead.unwrap_or(creator.id), "lead".into())]);
            let audits:i64=conn.query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",rusqlite::params![project.id,creator.id],|row|row.get(0)).unwrap();
            assert_eq!(audits, 1);
            let all_creation_audits: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create'",
                    [project.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(all_creation_audits, 1);
            drop(conn);
            let event = events.try_recv().unwrap();
            assert!(
                matches!(event.event,crate::realtime::RealtimeEvent::ProjectCreated{project_id} if project_id==project.id)
            );
            assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
            crate::authz::require_role(
                &db,
                &Some(crate::auth::fresh_identity(
                    if kind == "other" { &other } else { &creator },
                    Transport::Web,
                )),
                project.id,
                crate::db::models::Role::Lead,
            )
            .unwrap();
        }
    }
    #[test]
    fn creation_denials_and_foreign_key_failure_leave_no_rows_or_publication() {
        for kind in [
            "anonymous",
            "missing",
            "stale",
            "expired",
            "revoked",
            "wrong-user",
            "disabled",
            "deleted",
            "unknown-lead",
        ] {
            let (db, creator, other) = setup();
            let mut identity = Some(crate::auth::fresh_identity(&creator, Transport::Web));
            let mut token = None;
            let mut lead = Some(other.id);
            {
                let conn = db.write().unwrap();
                if matches!(
                    kind,
                    "stale" | "expired" | "revoked" | "wrong-user" | "unknown-lead"
                ) {
                    let session = queries::users::create_session(
                        &conn,
                        if kind == "wrong-user" {
                            other.id
                        } else {
                            creator.id
                        },
                        None,
                    )
                    .unwrap();
                    if kind == "stale" {
                        conn.execute(
                            "UPDATE sessions SET created_at=datetime('now','-1 hour')",
                            [],
                        )
                        .unwrap();
                    }
                    if kind == "expired" {
                        conn.execute(
                            "UPDATE sessions SET expires_at=datetime('now','-1 hour')",
                            [],
                        )
                        .unwrap();
                    }
                    if kind == "revoked" {
                        queries::users::delete_session(&conn, &session.token).unwrap();
                    }
                    token = Some(session.token);
                }
                if kind == "anonymous" {
                    identity = None;
                }
                if kind == "disabled" {
                    conn.execute("UPDATE users SET is_active=0 WHERE id=?1", [creator.id])
                        .unwrap();
                    lead = None;
                }
                if kind == "deleted" {
                    conn.execute("DELETE FROM users WHERE id=?1", [creator.id])
                        .unwrap();
                    lead = None;
                }
                if kind == "unknown-lead" {
                    lead = Some(999_999);
                }
            }
            let before = counts(&db);
            let hub = crate::realtime::RealtimeHub::new();
            let mut events = hub.subscribe();
            let result = create_project(&db, &hub, &identity, token.as_deref(), input(lead));
            if kind == "unknown-lead" {
                assert!(result.is_err());
            } else {
                assert!(
                    matches!(result, Err(LificError::Forbidden(_))),
                    "{kind}: {result:?}"
                );
            }
            assert_eq!(counts(&db), before, "{kind} rolls back all rows and audit");
            assert!(
                matches!(events.try_recv(), Err(TryRecvError::Empty)),
                "{kind} must not publish"
            );
        }
    }
    #[test]
    fn bot_creation_defaults_to_owner_and_cannot_grant_another_lead_with_human_session() {
        for explicit_owner in [false, true] {
            let (db, owner, other) = setup();
            let bot = queries::users::create_bot_user(
                &db.write().unwrap(),
                owner.id,
                "tool-bot",
                "Tool Bot",
                None,
            )
            .unwrap();
            let identity = Some(crate::auth::fresh_identity(&bot, Transport::Api));
            let hub = crate::realtime::RealtimeHub::new();
            let mut events = hub.subscribe();
            let project = create_project(
                &db,
                &hub,
                &identity,
                None,
                input(if explicit_owner { Some(owner.id) } else { None }),
            )
            .unwrap();
            assert_eq!(project.lead_user_id, Some(owner.id));
            let conn = db.read().unwrap();
            let members: Vec<(i64, String)> = {
                let mut statement = conn
                    .prepare("SELECT user_id,role FROM project_members WHERE project_id=?1")
                    .unwrap();
                statement
                    .query_map([project.id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap()
            };
            assert_eq!(members, vec![(owner.id, "lead".into())]);
            drop(conn);
            assert!(
                matches!(events.try_recv().unwrap().event,crate::realtime::RealtimeEvent::ProjectCreated{project_id} if project_id==project.id)
            );
            assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
            let token = queries::users::create_session(&db.write().unwrap(), owner.id, None)
                .unwrap()
                .token;
            let before = counts(&db);
            let mut other_input = input(Some(other.id));
            other_input.identifier = "OTHER".into();
            assert!(matches!(
                create_project(&db, &hub, &identity, Some(&token), other_input),
                Err(LificError::Forbidden(_))
            ));
            assert_eq!(counts(&db), before);
            assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        }
    }
}
