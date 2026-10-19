//! Shared GitHub import admissions, collection and database application.
use crate::{
    db::DbPool,
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Request body for `POST /api/projects/{id}/import/github`.
///
/// The web Import panel posts repo + token + mapping here. `dry_run` drives the
/// preview step (counts only, no writes). Only GitHub is exposed on the web;
/// Linear/Jira are CLI-only per LIF-265.
#[derive(serde::Deserialize)]
pub(crate) struct GithubImportRequest {
    /// Source repo as `owner/name`.
    pub(crate) repo: String,
    /// Optional GitHub token. Public repos work without one (subject to the
    /// anon rate limit).
    #[serde(default)]
    pub(crate) token: Option<String>,
    /// open / closed / all. Defaults to all.
    #[serde(default = "default_import_state")]
    pub(crate) state: String,
    /// Lific status for open issues.
    #[serde(default = "default_map_open")]
    pub(crate) map_open: String,
    /// Lific status for closed issues.
    #[serde(default = "default_map_closed")]
    pub(crate) map_closed: String,
    /// Preview only — count, write nothing.
    #[serde(default)]
    pub(crate) dry_run: bool,
}

// Keep a small global ceiling while allowing unrelated projects to import in
// parallel. The per-project gate below is the important isolation boundary:
// one expensive import cannot make another import for the same project race
// its writes or consume unbounded resources.
const GITHUB_IMPORT_GLOBAL_LIMIT: usize = 4;
static GITHUB_IMPORT_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static GITHUB_IMPORT_PROJECT_SLOTS: OnceLock<Mutex<HashMap<i64, Weak<Semaphore>>>> =
    OnceLock::new();

pub(crate) fn github_import_permits(
    project_id: i64,
) -> Result<(OwnedSemaphorePermit, OwnedSemaphorePermit), LificError> {
    let project_slot = {
        let slots = GITHUB_IMPORT_PROJECT_SLOTS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut slots = slots
            .lock()
            .map_err(|_| LificError::Internal("GitHub import gate poisoned".into()))?;
        slots.retain(|_, slot| slot.strong_count() > 0);
        match slots.entry(project_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if let Some(slot) = entry.get().upgrade() {
                    slot
                } else {
                    let slot = Arc::new(Semaphore::new(1));
                    entry.insert(Arc::downgrade(&slot));
                    slot
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let slot = Arc::new(Semaphore::new(1));
                entry.insert(Arc::downgrade(&slot));
                slot
            }
        }
    };
    let project_permit = project_slot.try_acquire_owned().map_err(|_| {
        LificError::Conflict("a GitHub import is already running for this project".into())
    })?;
    let global_permit = GITHUB_IMPORT_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(GITHUB_IMPORT_GLOBAL_LIMIT)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| LificError::Conflict("too many GitHub imports are already running".into()))?;
    Ok((global_permit, project_permit))
}

fn default_import_state() -> String {
    "all".to_string()
}
fn default_map_open() -> String {
    "backlog".to_string()
}
fn default_map_closed() -> String {
    "done".to_string()
}

/// POST /api/projects/{id}/import/github — run (or preview) a GitHub import
/// into this project.
///
/// Synchronous for v1: the request blocks until the import completes and
/// returns the [`crate::import::ImportSummary`]. The fetch + DB work runs in a
/// `spawn_blocking` task because the importer uses the blocking reqwest client.
/// Progress is a spinner on the client; a real dry-run preview precedes the
/// write so the operator sees counts first. Gated on project-lead (same bar as
/// editing project structure).
///
/// The actual collect/apply is delegated to [`import_github_with`], which takes
/// the fetcher as a parameter so tests can stub the network entirely.
pub(crate) async fn run(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    project_id: i64,
    req: GithubImportRequest,
) -> Result<crate::import::ImportSummary, LificError> {
    crate::authz::require_role(db, identity, project_id, crate::db::models::Role::Lead)?;
    // Move both permits into the blocking closure. `spawn_blocking` cannot
    // stop a running blocking task when this request is cancelled; keeping
    // the permits in that closure prevents a cancelled request from releasing
    // admission while its network/DB work is still running.
    let (global_permit, project_permit) = github_import_permits(project_id)?;

    // Resolve the import-bot owner from the authenticated user (the bot is
    // owned by whoever ran the import), so audit provenance is correct. On a
    // dry run we skip bot creation entirely.
    let owner_id = identity.as_ref().map(|i| i.user.id);
    let dry_run = req.dry_run;

    let db2 = db.clone();
    let summary = tokio::task::spawn_blocking(move || {
        let _global_permit = global_permit;
        let _project_permit = project_permit;
        run_github_import_blocking(&db2, project_id, owner_id, &req)
    })
    .await
    .map_err(|e| LificError::Internal(format!("import task failed: {e}")))??;

    if !dry_run {
        realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    }

    Ok(summary)
}

/// The blocking body of [`run`], factored out so it runs off the
/// async runtime (blocking reqwest) and so tests can call the injectable
/// [`import_github_with`] variant directly.
fn run_github_import_blocking(
    db: &DbPool,
    project_id: i64,
    owner_id: Option<i64>,
    req: &GithubImportRequest,
) -> Result<crate::import::ImportSummary, LificError> {
    let (owner, name) =
        crate::import::github::parse_repo(&req.repo).map_err(LificError::BadRequest)?;
    let state =
        crate::import::github::StateFilter::parse(&req.state).map_err(LificError::BadRequest)?;
    let fetcher = crate::import::github::LiveGithub::new(&owner, &name, req.token.clone())?;
    let slug = format!("{owner}/{name}");
    import_github_with(db, project_id, owner_id, &fetcher, &slug, state, req)
}

/// Core import logic with the fetcher injected. `owner_id` is the human who
/// owns the import bot (comments are attributed to it); `None` (fresh install /
/// dry run) skips comment attribution. Shared by the live handler and tests.
pub(crate) fn import_github_with(
    db: &DbPool,
    project_id: i64,
    owner_id: Option<i64>,
    fetcher: &dyn crate::import::github::GithubFetcher,
    slug: &str,
    state: crate::import::github::StateFilter,
    req: &GithubImportRequest,
) -> Result<crate::import::ImportSummary, LificError> {
    // LIF-385: `map_open` / `map_closed` arrive as free text from the web
    // Import panel; reject a bad one with 400 up front instead of letting every
    // insert fail against the status CHECK constraint.
    let status_map = crate::import::StatusMap {
        open: req.map_open.parse().map_err(LificError::BadRequest)?,
        closed: req.map_closed.parse().map_err(LificError::BadRequest)?,
    };
    // A resource-ceiling refusal surfaces as 413 (see the `GithubImportError`
    // conversion in `crate::error`); GitHub being unreachable stays a 500.
    let fetched = crate::import::github::collect(fetcher, slug, state, &status_map)?;

    // A dry run never mints a bot or writes; a real run resolves/creates the
    // import bot owned by the requester.
    let bot = if req.dry_run {
        None
    } else {
        match owner_id {
            Some(owner) => Some(crate::import::ensure_import_bot(
                db,
                owner,
                "github",
                "GitHub Import",
            )?),
            None => None,
        }
    };

    crate::import::run_import(db, project_id, bot, &fetched, req.dry_run)
}
