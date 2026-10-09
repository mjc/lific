use super::*;
use crate::db::{models::*, queries};
use rusqlite::params;

fn fixture() -> (crate::db::DbPool, i64, i64) {
    let db = crate::db::open_memory().unwrap();
    let (project, issue) = {
        let conn = db.write().unwrap();
        conn.execute_batch("INSERT INTO users (username,email,password_hash,is_admin) VALUES
            ('admin','admin@t.local','x',1), ('alice','alice@t.local','x',0),
            ('bob','bob@t.local','x',0), ('outsider','out@t.local','x',0),
            ('inactive','inactive@t.local','x',0);
            UPDATE users SET is_active=0 WHERE username='inactive';
            INSERT INTO users (username,email,password_hash,is_bot,owner_id)
            SELECT 'bot','bot@t.local','x',1,id FROM users WHERE username='admin';
            INSERT OR REPLACE INTO instance_settings (id,allow_signup,authz_enforced) VALUES (1,0,1);") .unwrap();
        let project = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "ASN".into(),
                name: "Assignments".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        for name in ["alice", "bob", "inactive"] {
            let id: i64 = conn
                .query_row("SELECT id FROM users WHERE username=?1", [name], |r| {
                    r.get(0)
                })
                .unwrap();
            queries::members::upsert_member(&conn, project, id, Role::Viewer).unwrap();
        }
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project,
                title: "Assign me".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        (project, issue)
    };
    (db, project, issue)
}

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

#[test]
fn assignments_round_trip_states_and_reject_invalid_people_without_writes() {
    let (db, project, issue) = fixture();
    let conn = db.write().unwrap();
    assert_eq!(assignment(&conn, issue).unwrap(), Assignment::default());
    set_assignment(&conn, issue, project, &names(&["human"]), None).unwrap();
    assert_eq!(
        assignment(&conn, issue).unwrap(),
        Assignment {
            needs_human: true,
            assignees: vec![]
        }
    );
    set_assignment(
        &conn,
        issue,
        project,
        &names(&["@ALICE", "bob", "alice"]),
        None,
    )
    .unwrap();
    let assigned = assignment(&conn, issue).unwrap();
    assert!(assigned.needs_human);
    assert_eq!(
        assigned
            .assignees
            .iter()
            .map(|a| a.username.as_str())
            .collect::<Vec<_>>(),
        ["alice", "bob"]
    );
    for invalid in [
        &["bot"][..],
        &["outsider"],
        &["inactive"],
        &["missing"],
        &["human", "alice"],
    ] {
        assert!(
            set_assignment(&conn, issue, project, &names(invalid), None).is_err(),
            "{invalid:?}"
        );
        assert_eq!(assignment(&conn, issue).unwrap(), assigned);
    }
    set_assignment(&conn, issue, project, &names(&["admin"]), None).unwrap();
    set_assignment(&conn, issue, project, &[], None).unwrap();
    assert_eq!(assignment(&conn, issue).unwrap(), Assignment::default());
}

#[test]
fn assignment_noop_preserves_cursor_and_audit_and_transaction_rolls_back() {
    let (db, project, issue) = fixture();
    let conn = db.write().unwrap();
    set_assignment(&conn, issue, project, &names(&["alice"]), None).unwrap();
    let seq = queries::issue_seq(&conn, issue).unwrap();
    let audit = || {
        conn.query_row(
            "SELECT count(*) FROM audit_log WHERE issue_id=?1 AND field='assignee'",
            [issue],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    let count = audit();
    set_assignment(&conn, issue, project, &names(&["alice"]), None).unwrap();
    assert_eq!(queries::issue_seq(&conn, issue).unwrap(), seq);
    assert_eq!(audit(), count);
    conn.execute_batch("BEGIN").unwrap();
    set_assignment(&conn, issue, project, &names(&["human"]), None).unwrap();
    conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        assignment(&conn, issue).unwrap().assignees[0].username,
        "alice"
    );
    assert_eq!(queries::issue_seq(&conn, issue).unwrap(), seq);
    assert_eq!(audit(), count);
}

#[test]
fn assignment_batch_and_account_deletion_preserve_human_work() {
    let (db, project, issue) = fixture();
    let conn = db.write().unwrap();
    set_assignment(&conn, issue, project, &names(&["alice"]), None).unwrap();
    let batch = assignments_by_issue(&conn, &[issue, i64::MAX]).unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[&issue], assignment(&conn, issue).unwrap());
    assert!(assignments_by_issue(&conn, &[]).unwrap().is_empty());
    conn.execute("DELETE FROM users WHERE username=?1", params!["alice"])
        .unwrap();
    assert_eq!(
        assignment(&conn, issue).unwrap(),
        Assignment {
            needs_human: true,
            assignees: vec![]
        }
    );
    conn.execute("DELETE FROM issues WHERE id=?1", [issue])
        .unwrap();
    assert_eq!(assignment(&conn, issue).unwrap(), Assignment::default());
}

#[test]
fn assignment_me_requires_a_caller_and_access_disabled_still_rejects_bots() {
    let mut requested = names(&["me", "@ME", "bob"]);
    resolve_me(&mut requested, Some("alice")).unwrap();
    assert_eq!(requested, names(&["alice", "alice", "bob"]));
    assert!(resolve_me(&mut names(&["me"]), None).is_err());
    let (db, project, issue) = fixture();
    let conn = db.write().unwrap();
    conn.execute("UPDATE instance_settings SET authz_enforced=0", [])
        .unwrap();
    set_assignment(&conn, issue, project, &names(&["outsider"]), None).unwrap();
    assert!(set_assignment(&conn, issue, project, &names(&["bot"]), None).is_err());
}
