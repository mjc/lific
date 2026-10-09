//! Issue assignment storage and validation, ported from upstream master.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LificError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueAssignee {
    pub user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
}

/// The reserved name that marks an issue for any person.
pub(crate) const HUMAN: &str = "human";

/// An issue's assignment as read from the table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub needs_human: bool,
    pub assignees: Vec<IssueAssignee>,
}

/// What a write asks for, after names are resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Unassigned,
    Human,
    People(BTreeSet<i64>),
}

const SELECT: &str = "SELECT a.issue_id, a.user_id, u.username, u.display_name
       FROM issue_assignees a
       LEFT JOIN users u ON u.id = a.user_id";

fn push_row(assignment: &mut Assignment, user: Option<(i64, String, Option<String>)>) {
    assignment.needs_human = true;
    if let Some((user_id, username, display_name)) = user {
        assignment.assignees.push(IssueAssignee {
            user_id,
            username,
            display_name: display_name.filter(|name| !name.is_empty()),
        });
    }
}

type Row = (i64, Option<(i64, String, Option<String>)>);

fn read_row(row: &rusqlite::Row) -> rusqlite::Result<Row> {
    let issue_id: i64 = row.get(0)?;
    let user_id: Option<i64> = row.get(1)?;
    let user = match user_id {
        Some(id) => Some((
            id,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        )),
        None => None,
    };
    Ok((issue_id, user))
}

/// One issue's assignment.
pub(crate) fn assignment(conn: &Connection, issue_id: i64) -> Result<Assignment, LificError> {
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE a.issue_id = ?1 ORDER BY u.username"
    ))?;
    let mut out = Assignment::default();
    for row in stmt.query_map(params![issue_id], read_row)? {
        push_row(&mut out, row?.1);
    }
    Ok(out)
}

/// Assignments for many issues in one round trip. Unassigned issues have no
/// entry.
pub(crate) fn assignments_by_issue(
    conn: &Connection,
    issue_ids: &[i64],
) -> Result<HashMap<i64, Assignment>, LificError> {
    let mut by_issue: HashMap<i64, Assignment> = HashMap::new();
    if issue_ids.is_empty() {
        return Ok(by_issue);
    }
    let placeholders = super::placeholders(issue_ids.len());
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE a.issue_id IN ({placeholders}) ORDER BY u.username"
    ))?;
    for row in stmt.query_map(rusqlite::params_from_iter(issue_ids), read_row)? {
        let (issue_id, user) = row?;
        push_row(by_issue.entry(issue_id).or_default(), user);
    }
    Ok(by_issue)
}

/// Whether `user_id` can see `project_id`: an admin, a member, or anyone
/// while project authorization is off.
fn can_see_project(conn: &Connection, user_id: i64, project_id: i64) -> Result<bool, LificError> {
    if !crate::db::queries::settings::get(conn)?.authz_enforced {
        return Ok(true);
    }
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1 AND is_admin = 1)
             OR EXISTS(SELECT 1 FROM project_members WHERE user_id = ?1 AND project_id = ?2)",
        params![user_id, project_id],
        |row| row.get(0),
    )?)
}

/// An active human account that can see the project, by username.
fn resolve_person(conn: &Connection, name: &str, project_id: i64) -> Result<i64, LificError> {
    let name = name.trim().trim_start_matches('@');
    let found: Option<(i64, String, bool)> = conn
        .query_row(
            "SELECT id, username, is_bot FROM users
              WHERE username = ?1 COLLATE NOCASE AND is_active = 1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (id, username, is_bot) =
        found.ok_or_else(|| LificError::BadRequest(format!("no active user named '{name}'")))?;
    if is_bot {
        return Err(LificError::BadRequest(format!(
            "@{username} is an agent account; only people can be assigned. Leave the issue unassigned for agents"
        )));
    }
    if !can_see_project(conn, id, project_id)? {
        return Err(LificError::BadRequest(format!(
            "@{username} is not a member of this project"
        )));
    }
    Ok(id)
}

fn parse_target(
    conn: &Connection,
    names: &[String],
    project_id: i64,
) -> Result<Target, LificError> {
    let names: Vec<&str> = names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    let human = names.iter().any(|name| name.eq_ignore_ascii_case(HUMAN));
    match (human, names.len()) {
        (_, 0) => Ok(Target::Unassigned),
        (true, 1) => Ok(Target::Human),
        (true, _) => Err(LificError::BadRequest(
            "assign either \"human\" (any person) or usernames, not both".into(),
        )),
        (false, _) => names
            .iter()
            .map(|name| resolve_person(conn, name, project_id))
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Target::People),
    }
}

/// Replace an issue's assignment. `names` is `[]` (unassigned), `["human"]`
/// or usernames; `me` must already be resolved. Only changed rows are
/// written, so re-sending the current assignment is a no-op that neither
/// bumps the issue's seq nor writes an audit entry.
pub(crate) fn set_assignment(
    conn: &Connection,
    issue_id: i64,
    project_id: i64,
    names: &[String],
    created_by: Option<i64>,
) -> Result<(), LificError> {
    let target = parse_target(conn, names, project_id)?;
    let current = assignment(conn, issue_id)?;
    let current_people: BTreeSet<i64> = current.assignees.iter().map(|a| a.user_id).collect();
    let current_human = current.needs_human && current_people.is_empty();

    let (want_human, want_people) = match target {
        Target::Unassigned => (false, BTreeSet::new()),
        Target::Human => (true, BTreeSet::new()),
        Target::People(people) => (false, people),
    };
    if current_human && !want_human {
        conn.execute(
            "DELETE FROM issue_assignees WHERE issue_id = ?1 AND user_id IS NULL",
            params![issue_id],
        )?;
    }
    for gone in current_people.difference(&want_people) {
        conn.execute(
            "DELETE FROM issue_assignees WHERE issue_id = ?1 AND user_id = ?2",
            params![issue_id, gone],
        )?;
    }
    if want_human && !current_human {
        conn.execute(
            "INSERT INTO issue_assignees (issue_id, user_id, created_by) VALUES (?1, NULL, ?2)",
            params![issue_id, created_by],
        )?;
    }
    for added in want_people.difference(&current_people) {
        conn.execute(
            "INSERT INTO issue_assignees (issue_id, user_id, created_by) VALUES (?1, ?2, ?3)",
            params![issue_id, added, created_by],
        )?;
    }
    Ok(())
}

/// Replace `me` (any case, with or without `@`) with the caller's username.
/// A request that says `me` without a caller is refused rather than guessed.
pub(crate) fn resolve_me(names: &mut [String], caller: Option<&str>) -> Result<(), LificError> {
    for name in names.iter_mut() {
        if name
            .trim()
            .trim_start_matches('@')
            .eq_ignore_ascii_case("me")
        {
            *name = caller
                .ok_or_else(|| {
                    LificError::BadRequest(
                        "\"me\" needs a signed-in caller; pass a username".into(),
                    )
                })?
                .to_string();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
