//! Retained personal ordering assertions against real cookie/service/SQLite writes.
use super::super::home_fixture;
use crate::db::{
    models::{CreateProject, CreateProjectGroup},
    queries,
};
use serde::Deserialize;
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Reset {
        id: usize,
    },
    Fault {
        id: usize,
        kind: String,
        enabled: bool,
    },
    Inspect {
        id: usize,
    },
}

async fn browser(required: bool) {
    let fixture = home_fixture::fixture_with_auth(required);
    let (actor, other, other_token, ids, groups, before) = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let other = queries::users::get_user_by_username(&conn, "lead")
            .unwrap()
            .id;
        let one = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute("UPDATE projects SET name='One' WHERE id=?1", [one])
            .unwrap();
        let mut ids = vec![one];
        for (identifier, name) in [("TWO", "Two"), ("THREE", "Three"), ("FOUR", "Four")] {
            let project = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: identifier.into(),
                    name: name.into(),
                    lead_user_id: Some(actor),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::members::add_member(&conn, project.id, other, "viewer").unwrap();
            ids.push(project.id);
        }
        let mut groups = Vec::new();
        for user in [actor, other] {
            let work = queries::project_groups::create_group(
                &conn,
                user,
                &CreateProjectGroup {
                    name: "Work".into(),
                },
            )
            .unwrap();
            let personal = queries::project_groups::create_group(
                &conn,
                user,
                &CreateProjectGroup {
                    name: "Personal".into(),
                },
            )
            .unwrap();
            for project in &ids[..2] {
                queries::project_groups::assign_project(&conn, user, *project, Some(work.id))
                    .unwrap();
            }
            // Preserve original fixture: Third is Personal; Fourth ungrouped.
            queries::project_groups::assign_project(&conn, user, ids[2], Some(personal.id))
                .unwrap();
            queries::reorder_projects(&conn, user, &ids, &Some(ids.iter().copied().collect()))
                .unwrap();
            groups.push([work.id, personal.id]);
        }
        let other_token = queries::users::create_session(&conn, other, None)
            .unwrap()
            .token;
        (
            actor,
            other,
            other_token,
            ids,
            groups,
            serde_json::to_value(queries::list_projects(&conn).unwrap()).unwrap(),
        )
    };
    let mut events = fixture.realtime.subscribe();
    let mut changed = [0_usize; 2];
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/order.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(serde_json::json!({"account":actor,"other_token":other_token,"ids":ids,"groups":groups,"auth_required":required}).to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        stderr.read_to_string(&mut text).await.unwrap();
        text
    });
    let mut output = String::new();
    let result=tokio::time::timeout(Duration::from_secs(240),async {
        while let Some(line)=lines.next_line().await.unwrap() {
            if let Some(json)=line.strip_prefix("ORDER_CONTROL ") {
                let control:Control=serde_json::from_str(json).unwrap();
                while let Ok(message)=events.try_recv() {
                    let at=match &message.event {
                        crate::realtime::RealtimeEvent::ProjectGroupsChanged=>Some(0),
                        crate::realtime::RealtimeEvent::ProjectsReordered=>Some(1),
                        _=>None,
                    };
                    if let Some(at)=at {
                        let conn=fixture.db.read().unwrap();
                        let owner=crate::auth::fresh_auth_user(&queries::users::get_user_by_id(&conn,actor).unwrap());
                        let second=crate::auth::fresh_auth_user(&queries::users::get_user_by_id(&conn,other).unwrap());
                        assert!(matches!(crate::realtime::visible_to(&fixture.db,&owner,&message),crate::realtime::EventVisibility::Visible));
                        assert!(matches!(crate::realtime::visible_to(&fixture.db,&second,&message),crate::realtime::EventVisibility::Hidden),"Personal reorder events target only their actual non-admin owner.");
                        changed[at]+=1;
                    }
                }
                let response={let conn=fixture.db.write().unwrap();match control {
                    Control::Reset{id}=>{
                        conn.execute_batch("DROP TRIGGER IF EXISTS native_sidebar_order_fault").unwrap();
                        queries::project_groups::reorder_groups(&conn,actor,&groups[0]).unwrap();
                        queries::reorder_projects(&conn,actor,&ids,&Some(ids.iter().copied().collect())).unwrap();
                        changed=[0;2];serde_json::json!({"id":id})
                    }
                    Control::Fault{id,kind,enabled}=>{
                        conn.execute_batch("DROP TRIGGER IF EXISTS native_sidebar_order_fault").unwrap();
                        if enabled {
                            // Move-down reverses the first two IDs. Each trigger
                            // rejects rank 1, after the preceding rank was written.
                            let sql=match kind.as_str() {
                                "group"=>format!("CREATE TRIGGER native_sidebar_order_fault BEFORE UPDATE OF sort_order ON project_groups WHEN NEW.user_id={actor} AND NEW.sort_order=1 BEGIN SELECT RAISE(ABORT,'sidebar later group rank fixture failure'); END;"),
                                "project"=>format!("CREATE TRIGGER native_sidebar_order_fault BEFORE INSERT ON user_project_order WHEN NEW.user_id={actor} AND NEW.sort_order=1 BEGIN SELECT RAISE(ABORT,'sidebar later project rank fixture failure'); END;"),
                                _=>panic!("invalid fixture kind"),
                            };
                            conn.execute_batch(&sql).unwrap();
                        }
                        serde_json::json!({"id":id})
                    }
                    Control::Inspect{id}=>{
                        let owned=|user| queries::project_groups::list_groups(&conn,user).unwrap().into_iter().map(|group| {
                            let mut members=group.project_ids;members.sort_unstable();
                            serde_json::json!({"id":group.id,"rank":group.sort_order,"projects":members})
                        }).collect::<Vec<_>>();
                        let order=|user| queries::list_projects_for_user(&conn,user).unwrap().into_iter().map(|project|project.id).filter(|id|ids.contains(id)).collect::<Vec<_>>();
                        serde_json::json!({"id":id,"projects":serde_json::to_value(queries::list_projects(&conn).unwrap()).unwrap(),"actor_order":order(actor),"other_order":order(other),"actor_groups":owned(actor),"other_groups":owned(other),"events":changed})
                    }
                }};
                input.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            }else{output.push_str(&line);output.push('\n');}
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            child.kill().await.unwrap();
            panic!(
                "Sidebar order timed out; auth required={required}:{error}\n{output}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Actual personal sidebar order; auth required={required}:\n{output}\n{}",
        errors.await.unwrap()
    );
    assert_eq!(
        serde_json::to_value(queries::list_projects(&fixture.db.read().unwrap()).unwrap()).unwrap(),
        before,
        "Personal reorder does not edit project records."
    );
}

#[tokio::test]
async fn native_sidebar_adapts_master_personal_group_and_project_order_payloads_rollback() {
    browser(true).await;
}

#[tokio::test]
async fn native_sidebar_adapts_master_personal_group_and_project_order_payloads_rollback_auth_optional()
 {
    browser(false).await;
}
