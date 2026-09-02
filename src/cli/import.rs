//! `lific import <source>` — CLI entry for the importers (LIF-264 / LIF-265).
//!
//! Thin glue: validate args, resolve the destination project + import-bot
//! owner, build the live fetcher, collect + normalize, then hand off to the
//! shared [`crate::import::run_import`]. Output is human-readable on a TTY and
//! JSON when piped (matching the rest of the CLI).

use crate::config::Config;
use crate::db::{self, DbPool};
use crate::import::beads;
use crate::import::github::{self, StateFilter};
use crate::import::jira::{self, JiraStatusMap};
use crate::import::linear::{self, LinearStatusMap};
use crate::import::{self, ImportSummary};

/// Dispatch an `ImportAction` to the right source runner.
pub fn run(
    cfg: &Config,
    action: &crate::cli::ImportAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::cli::ImportAction;
    let pool = db::open(&cfg.database.path)?;
    // CLI mutations run outside any request; attribute them via the CLI
    // transport (parity with `connect` / CRUD commands).
    crate::actor::set_default_transport(crate::actor::Transport::Cli);

    let summary = match action {
        ImportAction::Beads {
            path,
            source,
            project,
            user,
            dry_run,
        } => run_beads(&pool, path, source, project, user.as_deref(), *dry_run)?,
        ImportAction::Github {
            repo,
            project,
            state,
            token,
            map_open,
            map_closed,
            user,
            dry_run,
        } => run_github(
            &pool,
            repo,
            project,
            state,
            token.as_deref(),
            map_open,
            map_closed,
            user.as_deref(),
            *dry_run,
        )?,
        ImportAction::Linear {
            team,
            project,
            token,
            user,
            dry_run,
        } => run_linear(
            &pool,
            team,
            project,
            token.as_deref(),
            user.as_deref(),
            *dry_run,
        )?,
        ImportAction::Jira {
            site,
            jira_project,
            project,
            email,
            token,
            user,
            dry_run,
        } => run_jira(
            &pool,
            site,
            jira_project,
            project,
            email.as_deref(),
            token.as_deref(),
            user.as_deref(),
            *dry_run,
        )?,
    };

    print_summary(&summary, json);
    Ok(())
}

fn run_beads(
    pool: &DbPool,
    path: &std::path::Path,
    source: &str,
    project: &str,
    user: Option<&str>,
    dry_run: bool,
) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let fetched = beads::collect(path, source)?;
    let project_id = resolve_project(pool, project)?;
    if dry_run {
        return Ok(import::run_import(pool, project_id, None, &fetched, true)?);
    }
    let has_comments = fetched
        .issues
        .iter()
        .any(|issue| !issue.comments.is_empty());
    if has_comments && import::resolve_owner(pool, user)?.is_none() {
        return Err(
            "Beads export contains comments but no human owner exists; pass --user <username>"
                .into(),
        );
    }
    let bot = resolve_bot(pool, "beads", "Beads Import", user)?;
    Ok(import::run_import(pool, project_id, bot, &fetched, false)?)
}

/// Resolve a project identifier to its id, or a friendly error.
fn resolve_project(pool: &DbPool, ident: &str) -> Result<i64, Box<dyn std::error::Error>> {
    let conn = pool.read()?;
    crate::db::queries::resolve_project_identifier(&conn, ident)
        .map_err(|_| format!("Lific project '{ident}' not found. Create it first.").into())
}

/// Resolve the import bot: pick an owner, create-or-reuse a `import-<src>-<owner>`
/// bot. On a fresh install (no humans), returns `None` — comments are skipped
/// (there is no identity to attribute them to yet).
fn resolve_bot(
    pool: &DbPool,
    source_slug: &str,
    display: &str,
    user: Option<&str>,
) -> Result<Option<i64>, Box<dyn std::error::Error>> {
    match import::resolve_owner(pool, user)? {
        Some(owner) => Ok(Some(import::ensure_import_bot(
            pool,
            owner,
            source_slug,
            display,
        )?)),
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_github(
    pool: &DbPool,
    repo: &str,
    project: &str,
    state: &str,
    token: Option<&str>,
    map_open: &str,
    map_closed: &str,
    user: Option<&str>,
    dry_run: bool,
) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let (owner, name) = github::parse_repo(repo)?;
    let state = StateFilter::parse(state)?;
    let project_id = resolve_project(pool, project)?;
    // LIF-385: the `--map-open` / `--map-closed` flags are free text; parse
    // them into `Status` here so a typo fails with a clear message before any
    // network work, rather than as a CHECK-constraint error mid-import.
    let status_map = import::StatusMap {
        open: map_open.parse()?,
        closed: map_closed.parse()?,
    };

    let bot = if dry_run {
        None
    } else {
        resolve_bot(pool, "github", "GitHub Import", user)?
    };

    let fetcher = github::LiveGithub::new(&owner, &name, token.map(|s| s.to_string()))?;
    let slug = format!("{owner}/{name}");
    let fetched = github::collect(&fetcher, &slug, state, &status_map)?;
    let summary = import::run_import(pool, project_id, bot, &fetched, dry_run)?;
    Ok(summary)
}

fn run_linear(
    pool: &DbPool,
    team: &str,
    project: &str,
    token: Option<&str>,
    user: Option<&str>,
    dry_run: bool,
) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let token = token
        .ok_or("Linear API key required: pass --token or set LINEAR_API_KEY")?
        .to_string();
    let project_id = resolve_project(pool, project)?;
    let map = LinearStatusMap::default();

    let bot = if dry_run {
        None
    } else {
        resolve_bot(pool, "linear", "Linear Import", user)?
    };

    let fetcher = linear::LiveLinear::new(team, token)?;
    let fetched = linear::collect(&fetcher, &map)?;
    let summary = import::run_import(pool, project_id, bot, &fetched, dry_run)?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn run_jira(
    pool: &DbPool,
    site: &str,
    jira_project: &str,
    project: &str,
    email: Option<&str>,
    token: Option<&str>,
    user: Option<&str>,
    dry_run: bool,
) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let email = email.ok_or("Jira email required: pass --email or set JIRA_EMAIL")?;
    let token = token.ok_or("Jira API token required: pass --token or set JIRA_API_TOKEN")?;
    let project_id = resolve_project(pool, project)?;
    let map = JiraStatusMap::default();

    let bot = if dry_run {
        None
    } else {
        resolve_bot(pool, "jira", "Jira Import", user)?
    };

    let fetcher = jira::LiveJira::new(site, jira_project, email, token)?;
    let fetched = jira::collect(&fetcher, fetcher.site_slug(), &map)?;
    let summary = import::run_import(pool, project_id, bot, &fetched, dry_run)?;
    Ok(summary)
}

/// Render the summary for humans (or JSON when piped).
pub fn print_summary(summary: &ImportSummary, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(summary).unwrap());
        return;
    }
    println!();
    if summary.dry_run {
        println!("  Dry run — nothing was written.");
        println!();
        println!("  Would create:");
        println!("    issues:   {}", summary.issues_created);
        println!("    comments: {}", summary.comments_planned);
        println!("    labels:   {}", summary.labels_planned);
        if summary.issues_skipped_existing > 0 {
            println!(
                "    (skipping {} already-imported issue(s))",
                summary.issues_skipped_existing
            );
        }
    } else {
        println!("  Imported:");
        println!("    issues:   {}", summary.issues_created);
        println!("    comments: {}", summary.comments_created);
        println!("    labels:   {}", summary.labels_created);
        if summary.issues_skipped_existing > 0 {
            println!(
                "    skipped:  {} already imported",
                summary.issues_skipped_existing
            );
        }
    }
    // What didn't come across.
    if summary.skipped_non_issues + summary.skipped_assignees + summary.skipped_other > 0 {
        println!();
        println!("  Not imported (by design):");
        if summary.skipped_non_issues > 0 {
            println!("    {} pull request(s)", summary.skipped_non_issues);
        }
        if summary.skipped_assignees > 0 {
            println!("    {} assignee reference(s)", summary.skipped_assignees);
        }
        if summary.skipped_other > 0 {
            println!(
                "    {} milestone/estimate/type reference(s)",
                summary.skipped_other
            );
        }
    }
    if summary.skipped_tombstones + summary.skipped_ephemeral + summary.skipped_templates > 0 {
        println!();
        println!("  Skipped source records:");
        if summary.skipped_tombstones > 0 {
            println!("    {} tombstone(s)", summary.skipped_tombstones);
        }
        if summary.skipped_ephemeral > 0 {
            println!("    {} ephemeral record(s)", summary.skipped_ephemeral);
        }
        if summary.skipped_templates > 0 {
            println!("    {} template(s)", summary.skipped_templates);
        }
    }
    if summary.relations_created
        + summary.relations_planned
        + summary.relations_existing
        + summary.relations_missing_endpoint
        + summary.relations_unsupported
        + summary.relations_self
        > 0
    {
        println!();
        println!("  Relations:");
        if summary.relations_created > 0 {
            println!("    created: {}", summary.relations_created);
        }
        if summary.relations_planned > 0 {
            println!("    planned: {}", summary.relations_planned);
        }
        if summary.relations_existing > 0 {
            println!("    existing: {}", summary.relations_existing);
        }
        if summary.relations_missing_endpoint > 0 {
            println!(
                "    missing endpoint: {}",
                summary.relations_missing_endpoint
            );
        }
        if summary.relations_unsupported > 0 {
            println!("    unsupported: {}", summary.relations_unsupported);
        }
        if summary.relations_self > 0 {
            println!("    self relation: {}", summary.relations_self);
        }
    }
    println!();
}
