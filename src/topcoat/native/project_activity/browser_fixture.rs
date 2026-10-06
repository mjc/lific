//! Real audit rows and a loopback-only refresh endpoint for browser characterization.
use super::super::home_fixture::{self, Fixture};
use crate::{
    db::{DbPool, queries},
    realtime::{RealtimeEvent, RealtimeHub},
};
use axum::{Json, Router, extract::State, routing::post};

#[derive(Clone)]
struct RefreshState {
    db: DbPool,
    realtime: RealtimeHub,
    project: i64,
    issue: i64,
    actor: i64,
    bot: i64,
}

pub(super) fn fixture() -> Fixture {
    let mut fixture = home_fixture::fixture();
    let state = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        conn.execute(
            "UPDATE users SET display_name='Mary Jane' WHERE id=?1",
            [actor],
        )
        .unwrap();
        let empty = queries::create_project(
            &conn,
            &crate::db::models::CreateProject {
                identifier: "EMP".into(),
                name: "Empty visible project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::members::upsert_member(&conn, empty.id, actor, crate::db::models::Role::Viewer)
            .unwrap();
        conn.execute("DELETE FROM audit_log WHERE project_id=?1", [empty.id])
            .unwrap();
        let bot = queries::users::create_bot_user(
            &conn,
            actor,
            "activity-build-agent",
            "Build Agent",
            None,
        )
        .unwrap()
        .id;
        let project = queries::list_projects(&conn)
            .unwrap()
            .into_iter()
            .find(|project| project.identifier == "ACC")
            .unwrap()
            .id;
        let issue = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let state = RefreshState {
            db: fixture.db.clone(),
            realtime: fixture.realtime.clone(),
            project,
            issue,
            actor,
            bot,
        };
        seed(&conn, &state);
        state
    };
    fixture.app = fixture.app.merge(
        Router::new()
            .route("/__native_project_activity_test/refresh", post(refresh))
            .with_state(state),
    );
    fixture
}

fn seed(conn: &rusqlite::Connection, state: &RefreshState) {
    let tx = conn.unchecked_transaction().unwrap();
    tx.execute("DELETE FROM audit_log WHERE project_id=?1", [state.project])
        .unwrap();
    // Insert oldest first so the last two rows exercise full and collapsed diffs.
    for index in 0..70 {
        let actor = match index % 7 {
            0 => None,
            1 | 2 => Some(state.bot),
            _ => Some(state.actor),
        };
        // Swap one bot/human pair so the newest title audit belongs to the bot.
        let actor = match index {
            65 => Some(state.actor),
            68 => Some(state.bot),
            _ => actor,
        };
        let transport = match actor {
            None => "system",
            Some(actor) if actor == state.bot => "mcp",
            Some(_) => "web",
        };
        let (field, old, new) = if index == 69 {
            let before = (0..12)
                .map(|line| format!("Shared prefix {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            let after = (0..8)
                .map(|line| format!("Shared suffix {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            (
                "description",
                format!("{before}\nOLD_DIFF_MARKER\n{after}"),
                format!("{before}\nNEW_DIFF_MARKER\n{after}"),
            )
        } else if index == 68 {
            (
                "title",
                "OLD_FULL_MARKER".to_owned(),
                "NEW_FULL_MARKER".to_owned(),
            )
        } else {
            (
                "title",
                format!("Old activity title {index}"),
                format!("Activity title {index}"),
            )
        };
        let timestamp = format!("2026-10-{:02} {:02}:00:00", 1 + index / 24, index % 24);
        tx.execute("INSERT INTO audit_log(ts,actor_user_id,transport,entity_type,entity_id,entity_label,project_id,issue_id,action,field,old_value,new_value) VALUES(?1,?2,?3,'issue',?4,'ACC-1',?5,?4,'update',?6,?7,?8)",rusqlite::params![timestamp,actor,transport,state.issue,state.project,field,old,new]).unwrap();
    }
    tx.commit().unwrap();
}

async fn refresh(
    State(state): State<RefreshState>,
    request: Option<Json<serde_json::Value>>,
) -> Json<serde_json::Value> {
    let reset = request.is_some_and(|request| {
        request.0.get("reset").and_then(serde_json::Value::as_bool) == Some(true)
    });
    let metadata = {
        let conn = state.db.write().unwrap();
        let added_id = if reset {
            seed(&conn, &state);
            None
        } else {
            conn.execute("INSERT INTO audit_log(ts,actor_user_id,transport,entity_type,entity_id,entity_label,project_id,issue_id,action,field,old_value,new_value) VALUES('2026-10-03 23:59:00',?1,'web','issue',?2,'ACC-1',?3,?2,'update','title','Old refreshed title','NEW_ACTIVITY_MARKER')",rusqlite::params![state.actor,state.issue,state.project]).unwrap();
            Some(conn.last_insert_rowid())
        };
        let (total, oldest_id): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),MIN(id) FROM audit_log WHERE project_id=?1",
                [state.project],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        serde_json::json!({"total":total,"added_id":added_id,"actor_user_id":state.actor,"bot_user_id":state.bot,"oldest_id":oldest_id})
    };
    state.realtime.send(RealtimeEvent::IssueUpdated {
        project_id: state.project,
        issue_id: state.issue,
    });
    Json(metadata)
}
