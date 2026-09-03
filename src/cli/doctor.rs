//! `lific doctor` — diagnostic command.
//!
//! Runs a series of checks over the local setup and prints a colored
//! green/yellow/red status per check, in the spirit of `gh auth status` and
//! `claude doctor`. Both humans and agents run this to confirm a setup works;
//! it is the anti-"silent setup bug" tool.
//!
//! Exit semantics: any `fail` makes the process exit non-zero (we return an
//! `Err` string so `main`'s `?` propagation sets the code). `warn` never fails
//! the run — a warn means "works, but you should know" (e.g. no config file, no
//! server running). Server-dependent checks are `skipped` (neither pass nor
//! fail) when nothing is listening, so `doctor` is useful offline.
//!
//! ## Check catalogue (exact semantics)
//!
//! 1. **config** — which config file is in use. Explicit `--config` / `./lific.toml`
//!    / `~/.config/lific/lific.toml` = pass. Built-in defaults (no file) = warn.
//!    A file that exists but fails to parse = fail; the already-computed
//!    resolution is reported without repeating candidate discovery.
//! 2. **database** — file present and readable without migrations. Missing file
//!    with a writable parent = warn ("created on first start"); unwritable
//!    parent = fail. An existing non-Lific database = fail. `--repair` opts
//!    into applying pending migrations through the normal database opener.
//! 3. **backups** — only when enabled. Dir missing = warn (server creates it);
//!    dir present but unwritable = fail; no backups yet = warn; otherwise pass
//!    with the most-recent backup age vs the configured interval.
//! 4. **server** — HTTP reachability of `http://{host}:{port}/api/health`
//!    (0.0.0.0 → 127.0.0.1). Not running = warn (doctor must work offline).
//! 5. **oauth_discovery** — `GET {base}/.well-known/oauth-protected-resource/mcp`
//!    → 200 + JSON containing `resource`. Skipped when the server is unreachable.
//! 6. **mcp** — `POST {base}/mcp` JSON-RPC `initialize`. No key → expect 401 with
//!    a `WWW-Authenticate` header (auth enforced, discovery advertised) = pass.
//!    With a key → expect 200 + a `serverInfo` result = pass; wrong key = fail.
//!    Skipped when the server is unreachable.
//! 7. **public_url** — only when `server.public_url` is set. `GET
//!    {public_url}/.well-known/oauth-protected-resource/mcp` reachable = pass;
//!    unreachable = warn (may be firewalled from this vantage point).

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::config::Config;

/// Outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
    /// The check could not run (e.g. server offline). Neither success nor
    /// failure — never affects the exit code.
    Skipped,
}

/// One check's result.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
}

impl Check {
    fn new(name: &str, status: Status, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status,
            detail: detail.into(),
        }
    }
}

/// The full report: the ordered checks plus the derived ok flag.
#[derive(Debug, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
    pub ok: bool,
}

impl Report {
    fn new(checks: Vec<Check>) -> Self {
        let ok = !checks.iter().any(|c| c.status == Status::Fail);
        Self { checks, ok }
    }

    /// Count of failed checks (drives the error message / exit code).
    pub fn fail_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == Status::Fail)
            .count()
    }

    /// Human-readable one-line summary, e.g. "5 passed, 1 warning, 1 skipped".
    pub fn summary(&self) -> String {
        let mut passed = 0;
        let mut warned = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for c in &self.checks {
            match c.status {
                Status::Pass => passed += 1,
                Status::Warn => warned += 1,
                Status::Fail => failed += 1,
                Status::Skipped => skipped += 1,
            }
        }
        let mut parts = vec![format!("{passed} passed")];
        if warned > 0 {
            parts.push(format!("{warned} warning{}", plural(warned)));
        }
        if failed > 0 {
            parts.push(format!("{failed} failed"));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} skipped"));
        }
        parts.join(", ")
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Resolve `0.0.0.0` / `::` (bind-all) to a loopback address a client can
/// actually connect to. Everything else passes through unchanged.
fn connect_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "127.0.0.1",
        other => other,
    }
}

/// The base URL a client on this machine would dial to reach the configured
/// server. Bind-all is rewritten to loopback and IPv6 literals are bracketed,
/// so a `[server] host = "::1"` yields `http://[::1]:PORT` rather than the
/// unparseable `http://::1:PORT`.
fn connect_base(cfg: &Config) -> String {
    format!(
        "http://{}:{}",
        crate::display_host(connect_host(&cfg.server.host)),
        cfg.server.port
    )
}

/// Run doctor with the already-selected configuration result. This keeps
/// malformed configuration diagnostic while letting the remaining checks run
/// against safe defaults, instead of reconstructing candidate discovery.
/// `repair` explicitly opts the database check into applying migrations.
pub async fn run_with_resolution(
    cfg: &Config,
    resolution: &Result<crate::config::ResolvedConfig, crate::config::ConfigError>,
    key: Option<String>,
    repair: bool,
    json: bool,
) -> Result<(), String> {
    let database_mode = if repair {
        DatabaseCheckMode::Repair
    } else {
        DatabaseCheckMode::ReadOnly
    };
    let report = build_report_with_resolution(cfg, resolution, key.as_deref(), database_mode).await;
    print_report(&report, json);
    if report.fail_count() > 0 {
        Err(format!("doctor: {} check(s) failed", report.fail_count()))
    } else {
        Ok(())
    }
}

/// Run every check and assemble the report. Split from the printing entry point so tests can
/// inspect the structured result without touching stdout. Uses the default
/// config search order for provenance (no explicit `--config`).
#[cfg_attr(not(test), allow(dead_code))]
pub async fn build_report(cfg: &Config, key: Option<&str>) -> Report {
    build_report_with_config_path(cfg, None, key).await
}

/// Like [`build_report`] but honors an explicit `--config` path when reporting
/// which config file is in use.
pub async fn build_report_with_config_path(
    cfg: &Config,
    explicit_config: Option<&Path>,
    key: Option<&str>,
) -> Report {
    let resolution = Config::resolve(explicit_config);
    build_report_with_resolution(cfg, &resolution, key, DatabaseCheckMode::ReadOnly).await
}

async fn build_report_with_resolution(
    cfg: &Config,
    resolution: &Result<crate::config::ResolvedConfig, crate::config::ConfigError>,
    key: Option<&str>,
    database_mode: DatabaseCheckMode,
) -> Report {
    let mut checks = Vec::new();

    checks.push(check_config(resolution));
    checks.push(check_database(cfg, database_mode));
    if let Some(c) = check_backups(cfg) {
        checks.push(c);
    }

    // Build the base URL a client would use to reach this server.
    let base = connect_base(cfg);

    // LIF-252/LIF-258: if no --key/LIFIC_API_KEY was given, fall back to a
    // token stored by `lific login` (env > keyring > file). We probe the
    // credential store keyed by the server's public_url when set, else the
    // loopback base, since that's how `lific login` would have keyed it.
    let cred_base = cfg.server.public_url.as_deref().unwrap_or(&base);
    let (effective_key, key_source): (
        Option<String>,
        Option<crate::cli::credentials::TokenSource>,
    ) = match key {
        Some(k) => (Some(k.to_string()), None),
        None => match crate::cli::credentials::load_with_source(cred_base) {
            Ok(Some((tok, src))) => (Some(tok), Some(src)),
            Ok(None) => (None, None),
            Err(error) => {
                checks.push(Check::new(
                    "credentials",
                    Status::Fail,
                    format!("failed to read stored credentials: {error}"),
                ));
                (None, None)
            }
        },
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok();

    let server_up = match &client {
        Some(c) => http_server_reachable(c, &base).await,
        None => None,
    };

    match (&client, &server_up) {
        (Some(c), Some(reachable)) => {
            checks.push(server_check_result(reachable));
            if reachable.reachable {
                checks.push(check_oauth_discovery(c, &base).await);
                let mut mcp_check = check_mcp(c, &base, effective_key.as_deref()).await;
                // Note where the credential came from when it was a stored
                // login token rather than an explicit --key/LIFIC_API_KEY.
                if let Some(src) = key_source {
                    mcp_check.detail = format!("{} (using {})", mcp_check.detail, src.label());
                }
                checks.push(mcp_check);
            } else {
                checks.push(Check::new(
                    "oauth_discovery",
                    Status::Skipped,
                    "server not reachable — skipped",
                ));
                checks.push(Check::new(
                    "mcp",
                    Status::Skipped,
                    "server not reachable — skipped",
                ));
            }
        }
        _ => {
            // Could not even build a client; report all HTTP checks as skipped.
            checks.push(Check::new(
                "server",
                Status::Warn,
                "could not build HTTP client — skipped",
            ));
            checks.push(Check::new("oauth_discovery", Status::Skipped, "skipped"));
            checks.push(Check::new("mcp", Status::Skipped, "skipped"));
        }
    }

    // public_url check only when configured.
    if let (Some(c), Some(url)) = (&client, cfg.server.public_url.as_deref()) {
        checks.push(check_public_url(c, url).await);
    }

    Report::new(checks)
}

// ── Check 1: config ──────────────────────────────────────────────────────

/// Report the already-computed configuration selection or its typed failure.
///
/// Doctor keeps running after a configuration error so it can report other
/// independent checks, but it never hides the failed selection behind defaults.
fn check_config(
    resolution: &Result<crate::config::ResolvedConfig, crate::config::ConfigError>,
) -> Check {
    match resolution {
        Err(error) => Check::new("config", Status::Fail, error.to_string()),
        Ok(resolved) if resolved.source == crate::config::ConfigSource::BuiltInDefault => {
            Check::new(
                "config",
                Status::Warn,
                "no lific.toml found — using built-in defaults (run `lific init`)",
            )
        }
        Ok(resolved) => Check::new(
            "config",
            Status::Pass,
            format!(
                "using {} ({})",
                resolved.path.as_deref().map_or_else(
                    || "built-in defaults".to_owned(),
                    |path| path.display().to_string()
                ),
                resolved.source.label()
            ),
        ),
    }
}

// ── Check 2: database ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum DatabaseCheckMode {
    ReadOnly,
    Repair,
}

fn check_database(cfg: &Config, mode: DatabaseCheckMode) -> Check {
    let path = &cfg.database.path;
    if !path.exists() {
        // Missing is fine if the server could create it; the deciding factor is
        // whether the parent directory is writable.
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let parent_writable = match parent {
            Some(p) => dir_is_writable(p),
            None => dir_is_writable(Path::new(".")),
        };
        return if parent_writable {
            Check::new(
                "database",
                Status::Warn,
                format!(
                    "{} does not exist yet — will be created on first start",
                    path.display()
                ),
            )
        } else {
            Check::new(
                "database",
                Status::Fail,
                format!(
                    "{} missing and its parent directory is not writable",
                    path.display()
                ),
            )
        };
    }

    match mode {
        DatabaseCheckMode::ReadOnly => check_database_read_only(path),
        DatabaseCheckMode::Repair => check_database_with_repair(path),
    }
}

fn check_database_read_only(path: &Path) -> Check {
    // Doctor must not run migrations or enable WAL, because those operations
    // mutate the instance being diagnosed.
    match inspect_database(path) {
        Ok(Some(version)) => Check::new(
            "database",
            Status::Pass,
            format!(
                "{} opens read-only (schema v{version}); no migrations run",
                path.display()
            ),
        ),
        Ok(None) => Check::new(
            "database",
            Status::Fail,
            format!("{} is not a Lific database", path.display()),
        ),
        Err(e) => Check::new(
            "database",
            Status::Fail,
            format!("{} failed to inspect read-only: {e}", path.display()),
        ),
    }
}

fn check_database_with_repair(path: &Path) -> Check {
    match crate::db::open(path) {
        Ok(pool) => match pool
            .read()
            .ok()
            .and_then(|connection| schema_version(&connection).ok())
        {
            Some(version) => Check::new(
                "database",
                Status::Pass,
                format!(
                    "{} opens; migrations applied (schema v{version})",
                    path.display()
                ),
            ),
            None => Check::new(
                "database",
                Status::Pass,
                format!("{} opens; migrations applied", path.display()),
            ),
        },
        Err(error) => Check::new(
            "database",
            Status::Fail,
            format!("{} failed to open: {error}", path.display()),
        ),
    }
}

fn schema_version(connection: &rusqlite::Connection) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
        [],
        |row| row.get::<_, i64>(0),
    )
}

/// Inspect the migration marker without creating WAL/SHM files or applying
/// migrations. `Ok(None)` means the file opened but is not a Lific database.
fn inspect_database(path: &Path) -> rusqlite::Result<Option<i64>> {
    use rusqlite::OptionalExtension;

    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let migration_table: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '_migrations'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(_) = migration_table else {
        return Ok(None);
    };
    schema_version(&connection).map(Some)
}

/// Best-effort writability probe: try to create (and remove) a temp file in the
/// directory. Returns false if the dir doesn't exist or the write is refused.
fn dir_is_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(".lific-doctor-write-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// ── Check 3: backups ─────────────────────────────────────────────────────

/// Returns `None` when backups are disabled (the check is omitted entirely).
fn check_backups(cfg: &Config) -> Option<Check> {
    if !cfg.backup.enabled {
        return None;
    }
    let dir = cfg.backup_dir();
    if !dir.is_dir() {
        // The server creates the dir on first backup, so a missing dir is only
        // a warning — unless we can't even reach the intended location's parent.
        return Some(Check::new(
            "backups",
            Status::Warn,
            format!(
                "backup dir {} does not exist yet — created on first backup",
                dir.display()
            ),
        ));
    }
    if !dir_is_writable(&dir) {
        return Some(Check::new(
            "backups",
            Status::Fail,
            format!("backup dir {} exists but is not writable", dir.display()),
        ));
    }

    match newest_backup_age_minutes(&dir) {
        Some(age_min) => {
            let interval = cfg.backup.interval_minutes;
            // A backup older than ~2 intervals suggests the backup task isn't
            // running; flag it. Otherwise it's healthy.
            if interval > 0 && age_min > interval.saturating_mul(2) {
                Some(Check::new(
                    "backups",
                    Status::Warn,
                    format!(
                        "most recent backup is {age_min}m old (interval {interval}m) — task may be stopped"
                    ),
                ))
            } else {
                Some(Check::new(
                    "backups",
                    Status::Pass,
                    format!("most recent backup {age_min}m old (interval {interval}m)"),
                ))
            }
        }
        None => Some(Check::new(
            "backups",
            Status::Warn,
            format!("{} is writable but has no backups yet", dir.display()),
        )),
    }
}

/// Age in minutes of the most recent backup artifact in the backup dir, or
/// `None` if there are none.
///
/// LIF-266: backups are now single `lific_*.tar.gz` archives. Legacy bare
/// `lific_*.db` snapshots from the pre-archive scheme are still counted so a
/// mixed backup dir mid-migration reports freshness correctly. Other files
/// (e.g. the legacy mirrored `attachments/` dir) are ignored.
fn newest_backup_age_minutes(dir: &Path) -> Option<u64> {
    let mut newest: Option<std::time::SystemTime> = None;
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_backup_artifact = name.ends_with(".tar.gz") || name.ends_with(".db");
        if !is_backup_artifact {
            continue;
        }
        if let Ok(meta) = entry.metadata()
            && meta.is_file()
            && let Ok(modified) = meta.modified()
        {
            newest = Some(match newest {
                Some(cur) if cur >= modified => cur,
                _ => modified,
            });
        }
    }
    let newest = newest?;
    let elapsed = std::time::SystemTime::now()
        .duration_since(newest)
        .unwrap_or_default();
    Some(elapsed.as_secs() / 60)
}

// ── Check 4: server reachability ─────────────────────────────────────────

/// Result of probing the server's health endpoint.
pub struct ServerProbe {
    pub reachable: bool,
    pub status: Option<u16>,
}

/// Probe `GET {base}/api/health`. `Some(probe)` always (reachable flags whether
/// a connection succeeded). Kept separate from the `Check` so the follow-on
/// HTTP checks can gate on `reachable` without re-parsing a detail string.
pub async fn http_server_reachable(client: &reqwest::Client, base: &str) -> Option<ServerProbe> {
    let url = format!("{}/api/health", base.trim_end_matches('/'));
    match client.get(&url).send().await {
        Ok(resp) => Some(ServerProbe {
            reachable: true,
            status: Some(resp.status().as_u16()),
        }),
        Err(_) => Some(ServerProbe {
            reachable: false,
            status: None,
        }),
    }
}

fn server_check_result(probe: &ServerProbe) -> Check {
    if probe.reachable {
        let ver = env!("CARGO_PKG_VERSION");
        Check::new(
            "server",
            Status::Pass,
            format!(
                "reachable (health {}); this binary is lific {ver}",
                probe.status.unwrap_or(0)
            ),
        )
    } else {
        Check::new(
            "server",
            Status::Warn,
            "not running (start it with `lific start`) — server checks skipped",
        )
    }
}

// ── Check 5: OAuth discovery ─────────────────────────────────────────────

/// `GET {base}/.well-known/oauth-protected-resource/mcp` → 200 + JSON with a
/// `resource` field.
pub async fn check_oauth_discovery(client: &reqwest::Client, base: &str) -> Check {
    let url = format!(
        "{}/.well-known/oauth-protected-resource/mcp",
        base.trim_end_matches('/')
    );
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                return Check::new(
                    "oauth_discovery",
                    Status::Fail,
                    format!("discovery endpoint returned HTTP {}", status.as_u16()),
                );
            }
            match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    if let Some(resource) = body.get("resource").and_then(|r| r.as_str()) {
                        Check::new(
                            "oauth_discovery",
                            Status::Pass,
                            format!("advertised, resource = {resource}"),
                        )
                    } else {
                        Check::new(
                            "oauth_discovery",
                            Status::Fail,
                            "200 but JSON is missing the `resource` field",
                        )
                    }
                }
                Err(e) => Check::new(
                    "oauth_discovery",
                    Status::Fail,
                    format!("200 but body was not JSON: {e}"),
                ),
            }
        }
        Err(e) => Check::new(
            "oauth_discovery",
            Status::Fail,
            format!("request failed: {e}"),
        ),
    }
}

// ── Check 6: MCP round-trip ──────────────────────────────────────────────

/// The JSON-RPC `initialize` request body. Protocol version pinned to the one
/// the server supports (`V_2025_03_26`).
fn initialize_body() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "lific-doctor", "version": env!("CARGO_PKG_VERSION") }
        }
    })
}

/// `POST {base}/mcp` an `initialize`. Without a key we expect a 401 carrying a
/// `WWW-Authenticate` header (auth enforced, discovery advertised). With a key
/// we expect a 200 whose JSON-RPC result contains `serverInfo`.
pub async fn check_mcp(client: &reqwest::Client, base: &str, key: Option<&str>) -> Check {
    let url = format!("{}/mcp", base.trim_end_matches('/'));
    let mut req = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .json(&initialize_body());
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return Check::new("mcp", Status::Fail, format!("request failed: {e}"));
        }
    };

    let status = resp.status();
    let has_www_auth = resp
        .headers()
        .contains_key(reqwest::header::WWW_AUTHENTICATE);

    match key {
        None => {
            // No key: the correct, healthy behavior is a 401 that advertises
            // where to discover auth.
            if status == reqwest::StatusCode::UNAUTHORIZED && has_www_auth {
                Check::new(
                    "mcp",
                    Status::Pass,
                    "auth enforced (401 + WWW-Authenticate); discovery advertised",
                )
            } else if status == reqwest::StatusCode::UNAUTHORIZED {
                Check::new(
                    "mcp",
                    Status::Warn,
                    "401 but no WWW-Authenticate header — discovery not advertised",
                )
            } else {
                Check::new(
                    "mcp",
                    Status::Fail,
                    format!(
                        "expected 401 without a key, got HTTP {} (auth may be disabled)",
                        status.as_u16()
                    ),
                )
            }
        }
        Some(_) => {
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Check::new(
                    "mcp",
                    Status::Fail,
                    "provided key was rejected (401) — wrong or revoked key",
                );
            }
            if !status.is_success() {
                return Check::new(
                    "mcp",
                    Status::Fail,
                    format!("initialize returned HTTP {}", status.as_u16()),
                );
            }
            // json_response mode: the body is a plain JSON-RPC envelope.
            match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    if body
                        .get("result")
                        .and_then(|r| r.get("serverInfo"))
                        .is_some()
                    {
                        let name = body
                            .pointer("/result/serverInfo/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("lific");
                        Check::new(
                            "mcp",
                            Status::Pass,
                            format!("authorized initialize succeeded (serverInfo: {name})"),
                        )
                    } else if body.get("error").is_some() {
                        Check::new(
                            "mcp",
                            Status::Fail,
                            format!("initialize returned a JSON-RPC error: {}", body["error"]),
                        )
                    } else {
                        Check::new("mcp", Status::Fail, "200 but result had no serverInfo")
                    }
                }
                Err(e) => Check::new(
                    "mcp",
                    Status::Fail,
                    format!("200 but body was not JSON: {e}"),
                ),
            }
        }
    }
}

// ── Check 7: public_url ──────────────────────────────────────────────────

async fn check_public_url(client: &reqwest::Client, public_url: &str) -> Check {
    let url = format!(
        "{}/.well-known/oauth-protected-resource/mcp",
        public_url.trim_end_matches('/')
    );
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => Check::new(
            "public_url",
            Status::Pass,
            format!("{public_url} reachable (discovery 200)"),
        ),
        Ok(resp) => Check::new(
            "public_url",
            Status::Warn,
            format!(
                "{public_url} returned HTTP {} — may be firewalled/misconfigured from here",
                resp.status().as_u16()
            ),
        ),
        Err(_) => Check::new(
            "public_url",
            Status::Warn,
            format!("{public_url} unreachable from this vantage (may be firewalled)"),
        ),
    }
}

// ── Output ───────────────────────────────────────────────────────────────

fn print_report(report: &Report, json: bool) {
    if json {
        // Machine output: stable shape for agents/scripts.
        match serde_json::to_string_pretty(report) {
            Ok(s) => println!("{s}"),
            Err(e) => println!("{{\"error\":\"failed to serialize report: {e}\"}}"),
        }
        return;
    }

    use crate::cli::ui;
    ui::intro("lific doctor");
    let name_width = report
        .checks
        .iter()
        .map(|c| c.name.len())
        .max()
        .unwrap_or(0);

    for c in &report.checks {
        let line = format!("{:<width$}  {}", c.name, c.detail, width = name_width);
        match c.status {
            Status::Pass => ui::step(line),
            Status::Warn => ui::warn(line),
            Status::Fail => ui::error(line),
            Status::Skipped => ui::skipped(line),
        }
    }
    if report.ok {
        ui::outro(report.summary());
    } else {
        ui::outro_cancel(report.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq};
    use rusqlite::OptionalExtension;

    fn check(name: &str, status: Status) -> Check {
        Check::new(name, status, "")
    }

    // ── Report / exit-logic ──────────────────────────────────────────────

    #[test]
    fn report_ok_when_no_fail() {
        let r = Report::new(vec![
            check("a", Status::Pass),
            check("b", Status::Warn),
            check("c", Status::Skipped),
        ]);
        assert!(r.ok);
        assert_eq!(r.fail_count(), 0);
    }

    #[test]
    fn report_not_ok_with_any_fail() {
        let r = Report::new(vec![check("a", Status::Pass), check("b", Status::Fail)]);
        assert!(!r.ok);
        assert_eq!(r.fail_count(), 1);
    }

    #[test]
    fn warn_and_skip_never_fail_the_run() {
        let r = Report::new(vec![
            check("a", Status::Warn),
            check("b", Status::Skipped),
            check("c", Status::Warn),
        ]);
        assert!(r.ok);
        assert_eq!(r.fail_count(), 0);
    }

    // ── Summary text ─────────────────────────────────────────────────────

    #[test]
    fn summary_counts_and_pluralizes() {
        let r = Report::new(vec![
            check("a", Status::Pass),
            check("b", Status::Pass),
            check("c", Status::Warn),
            check("d", Status::Skipped),
        ]);
        assert_eq!(r.summary(), "2 passed, 1 warning, 1 skipped");
    }

    #[test]
    fn summary_plural_warnings_and_fails() {
        let r = Report::new(vec![
            check("a", Status::Pass),
            check("b", Status::Warn),
            check("c", Status::Warn),
            check("d", Status::Fail),
        ]);
        assert_eq!(r.summary(), "1 passed, 2 warnings, 1 failed");
    }

    #[test]
    fn summary_all_pass() {
        let r = Report::new(vec![check("a", Status::Pass), check("b", Status::Pass)]);
        assert_eq!(r.summary(), "2 passed");
    }

    // ── JSON shape ───────────────────────────────────────────────────────

    #[test]
    fn json_shape_matches_spec() {
        let r = Report::new(vec![
            Check::new("config", Status::Pass, "using ./lific.toml"),
            Check::new("server", Status::Warn, "not running"),
            Check::new("mcp", Status::Skipped, "skipped"),
        ]);
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["ok"], serde_json::json!(true));
        let checks = v["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0]["name"], "config");
        assert_eq!(checks[0]["status"], "pass");
        assert_eq!(checks[0]["detail"], "using ./lific.toml");
        assert_eq!(checks[1]["status"], "warn");
        assert_eq!(checks[2]["status"], "skipped");
    }

    #[test]
    fn json_ok_false_when_failing() {
        let r = Report::new(vec![Check::new("database", Status::Fail, "boom")]);
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["ok"], serde_json::json!(false));
        assert_eq!(v["checks"][0]["status"], "fail");
    }

    // ── connect_host ─────────────────────────────────────────────────────

    #[test]
    fn connect_host_rewrites_bind_all() {
        assert_eq!(connect_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(connect_host("::"), "127.0.0.1");
        assert_eq!(connect_host("[::]"), "127.0.0.1");
        assert_eq!(connect_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(connect_host("example.com"), "example.com");
    }

    #[test]
    fn connect_base_brackets_ipv6_literal_hosts() {
        let mut cfg = Config::default();
        cfg.server.host = "::1".into();
        cfg.server.port = 7777;
        assert_eq!(connect_base(&cfg), "http://[::1]:7777");

        cfg.server.host = "fd00::5".into();
        assert_eq!(connect_base(&cfg), "http://[fd00::5]:7777");
    }

    #[test]
    fn connect_base_leaves_ipv4_and_hostnames_bare() {
        let mut cfg = Config::default();
        cfg.server.port = 3456;

        cfg.server.host = "127.0.0.1".into();
        assert_eq!(connect_base(&cfg), "http://127.0.0.1:3456");

        cfg.server.host = "tracker.example".into();
        assert_eq!(connect_base(&cfg), "http://tracker.example:3456");

        // Bind-all still rewrites to loopback before bracketing applies.
        cfg.server.host = "::".into();
        assert_eq!(connect_base(&cfg), "http://127.0.0.1:3456");
    }

    // ── config check (provenance + parse-error surfacing) ────────────────

    #[test]
    fn config_check_fails_on_unparseable_file() {
        // A file that exists but is broken TOML must surface as a FAIL rather
        // than aborting the run. We exercise the parse branch directly.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("bad.toml");
        std::fs::write(&path, "{{{ not toml").unwrap();

        // Reproduce the inner parse logic the check uses.
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed = toml::from_str::<Config>(&contents);
        assert!(parsed.is_err(), "broken toml must fail to parse");
    }

    #[test]
    fn config_check_warns_for_built_in_defaults() {
        let resolution = crate::config::ResolvedConfig {
            config: Config::default(),
            path: None,
            source: crate::config::ConfigSource::BuiltInDefault,
        };

        let check = check_config(&Ok(resolution));

        assert_eq!(check.status, Status::Warn);
        assert!(check.detail.contains("built-in defaults"));
    }

    proptest::proptest! {
        #[test]
        fn every_explicit_config_failure_is_reported_as_a_fail(
            name in "[a-zA-Z0-9_-]{1,24}"
        ) {
            let error = crate::config::ConfigError::MissingExplicit {
                path: std::path::PathBuf::from(format!("{name}.toml")),
            };

            let check = check_config(&Err(error));

            prop_assert_eq!(check.status, Status::Fail);
            prop_assert!(check.detail.contains("does not exist"));
        }
    }

    // ── database check ───────────────────────────────────────────────────

    #[test]
    fn database_check_warns_when_missing_but_parent_writable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut cfg = Config::default();
        cfg.database.path = dir.join("nope.db");

        let c = check_database(&cfg, DatabaseCheckMode::ReadOnly);
        assert_eq!(c.status, Status::Warn, "detail: {}", c.detail);
        assert!(c.detail.contains("first start"));
    }

    #[test]
    fn database_check_fails_when_parent_unwritable() {
        let mut cfg = Config::default();
        // A path under a directory that does not exist → parent not writable.
        cfg.database.path = std::path::PathBuf::from("/nonexistent-lific-doctor-xyz/deep/lific.db");
        let c = check_database(&cfg, DatabaseCheckMode::ReadOnly);
        assert_eq!(c.status, Status::Fail, "detail: {}", c.detail);
    }

    #[test]
    fn database_check_passes_on_real_db() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let db_path = dir.join("lific.db");
        // Create a real migrated DB.
        let _pool = crate::db::open(&db_path).unwrap();

        let mut cfg = Config::default();
        cfg.database.path = db_path;
        let c = check_database(&cfg, DatabaseCheckMode::ReadOnly);
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
        assert!(c.detail.contains("no migrations run"));
    }

    #[test]
    fn database_check_does_not_migrate_an_old_database() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("old.db");
        create_unrecognized_database(&db_path);

        let mut cfg = Config::default();
        cfg.database.path = db_path.clone();
        let c = check_database(&cfg, DatabaseCheckMode::ReadOnly);

        assert_eq!(c.status, Status::Fail, "an unrecognized database must fail");
        assert!(!has_migration_table(&db_path));
        assert_no_database_sidecars(&db_path);
    }

    proptest::proptest! {
        #[test]
        fn read_only_database_checks_never_repair_arbitrary_paths(
            name in "[a-zA-Z0-9_-]{1,24}"
        ) {
            let tmp = tempfile::tempdir().unwrap();
            let db_path = tmp.path().join(format!("{name}.db"));
            create_unrecognized_database(&db_path);

            let mut cfg = Config::default();
            cfg.database.path = db_path.clone();

            let check = check_database(&cfg, DatabaseCheckMode::ReadOnly);

            prop_assert_eq!(check.status, Status::Fail);
            prop_assert!(!has_migration_table(&db_path));
            assert_no_database_sidecars(&db_path);
        }
    }

    #[test]
    fn database_check_repairs_an_old_database_only_when_requested() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("old.db");
        create_unrecognized_database(&db_path);

        let mut cfg = Config::default();
        cfg.database.path = db_path.clone();

        let read_only = check_database(&cfg, DatabaseCheckMode::ReadOnly);
        assert_eq!(read_only.status, Status::Fail);
        assert!(
            !has_migration_table(&db_path),
            "read-only doctor must not repair"
        );
        assert_no_database_sidecars(&db_path);

        let repaired = check_database(&cfg, DatabaseCheckMode::Repair);
        assert_eq!(repaired.status, Status::Pass, "detail: {}", repaired.detail);
        assert!(repaired.detail.contains("migrations applied"));
    }

    fn has_migration_table(path: &Path) -> bool {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '_migrations'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .unwrap()
            .is_some()
    }

    fn create_unrecognized_database(path: &Path) {
        rusqlite::Connection::open(path)
            .unwrap()
            .execute("CREATE TABLE marker (value TEXT NOT NULL)", [])
            .unwrap();
    }

    fn assert_no_database_sidecars(path: &Path) {
        assert!(!path.with_extension("db-wal").exists());
        assert!(!path.with_extension("db-shm").exists());
        assert!(!path.with_extension("db.bak").exists());
    }

    // ── backups check ────────────────────────────────────────────────────

    #[test]
    fn backups_check_none_when_disabled() {
        let mut cfg = Config::default();
        cfg.backup.enabled = false;
        assert!(check_backups(&cfg).is_none());
    }

    #[test]
    fn backups_check_warns_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut cfg = Config::default();
        cfg.backup.enabled = true;
        cfg.database.path = dir.join("lific.db");
        cfg.backup.dir = std::path::PathBuf::from("backups"); // resolves under dir, absent
        let c = check_backups(&cfg).unwrap();
        assert_eq!(c.status, Status::Warn, "detail: {}", c.detail);
        assert!(c.detail.contains("first backup"));
    }

    #[test]
    fn backups_check_warns_when_empty_but_writable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bkdir = dir.join("backups");
        std::fs::create_dir_all(&bkdir).unwrap();
        let mut cfg = Config::default();
        cfg.backup.enabled = true;
        cfg.database.path = dir.join("lific.db");
        cfg.backup.dir = std::path::PathBuf::from("backups");
        let c = check_backups(&cfg).unwrap();
        assert_eq!(c.status, Status::Warn, "detail: {}", c.detail);
        assert!(c.detail.contains("no backups yet"));
    }

    #[test]
    fn backups_check_passes_with_recent_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bkdir = dir.join("backups");
        std::fs::create_dir_all(&bkdir).unwrap();
        std::fs::write(bkdir.join("lific-20260101.db"), b"x").unwrap();
        let mut cfg = Config::default();
        cfg.backup.enabled = true;
        cfg.backup.interval_minutes = 60;
        cfg.database.path = dir.join("lific.db");
        cfg.backup.dir = std::path::PathBuf::from("backups");
        let c = check_backups(&cfg).unwrap();
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
    }

    #[test]
    fn backups_check_recognizes_tar_gz_archives() {
        // LIF-266: the interval task now writes `lific_*.tar.gz` archives.
        // Freshness must be reported against the new naming.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bkdir = dir.join("backups");
        std::fs::create_dir_all(&bkdir).unwrap();
        std::fs::write(bkdir.join("lific_20260101_120000.tar.gz"), b"x").unwrap();
        let mut cfg = Config::default();
        cfg.backup.enabled = true;
        cfg.backup.interval_minutes = 60;
        cfg.database.path = dir.join("lific.db");
        cfg.backup.dir = std::path::PathBuf::from("backups");
        let c = check_backups(&cfg).unwrap();
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
    }

    #[test]
    fn backups_check_ignores_non_artifact_files() {
        // A backup dir containing only a leftover mirrored `attachments/` dir
        // (and no archive) must still report "no backups yet", not treat the
        // dir's contents as a fresh backup.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bkdir = dir.join("backups");
        std::fs::create_dir_all(bkdir.join("attachments")).unwrap();
        std::fs::write(bkdir.join("attachments").join("deadbeef"), b"blob").unwrap();
        let mut cfg = Config::default();
        cfg.backup.enabled = true;
        cfg.backup.interval_minutes = 60;
        cfg.database.path = dir.join("lific.db");
        cfg.backup.dir = std::path::PathBuf::from("backups");
        let c = check_backups(&cfg).unwrap();
        assert_eq!(c.status, Status::Warn, "detail: {}", c.detail);
        assert!(c.detail.contains("no backups yet"));
    }

    // ── server probe (offline) ───────────────────────────────────────────

    #[tokio::test]
    async fn server_probe_reports_unreachable_when_nothing_listens() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap();
        // Port 1 is privileged and nothing listens there in test.
        let probe = http_server_reachable(&client, "http://127.0.0.1:1")
            .await
            .unwrap();
        assert!(!probe.reachable);
        let c = server_check_result(&probe);
        assert_eq!(c.status, Status::Warn);
    }

    // ── HTTP integration: spin up a real server in-process ───────────────
    //
    // LIF-390: this used to hand-assemble a lookalike router, which drifted
    // from production (it ran `with_request_user` instead of
    // `with_request_context` and wired no realtime hub, so doctor validated a
    // stack nothing served). It now calls `server::build_app` — the exact
    // router `lific start` builds — binds it on an ephemeral loopback port,
    // and runs the doctor HTTP check functions against it. This proves the
    // three paths that matter: 401-with-WWW-Authenticate (no key), discovery
    // 200, and an authorized `initialize` round-trip with a real key.

    use std::net::SocketAddr;
    use std::sync::Arc;

    /// Build the production app for an instance whose issuer is `issuer`.
    fn build_test_app(pool: crate::db::DbPool, issuer: &str) -> axum::Router {
        let mut cfg = Config::default();
        // An explicit public_url is what makes the issuer authoritative
        // (`issuer_is_explicit`), matching a deployed instance.
        cfg.server.public_url = Some(issuer.to_string());
        let trusted_proxies = Arc::<[crate::ratelimit::IpNetwork]>::from(
            cfg.server
                .trusted_proxy_ranges()
                .expect("default trusted proxy ranges must parse"),
        );
        crate::server::build_app(
            &cfg,
            pool,
            crate::auth::create_key_manager().unwrap(),
            crate::realtime::RealtimeHub::new(),
            trusted_proxies,
        )
    }

    /// Bind an ephemeral loopback port, serve `app`, and return the base URL.
    async fn serve_ephemeral(app: axum::Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // Connect info is part of the production service: the login and
            // OAuth registration handlers extract `ConnectInfo<SocketAddr>`
            // for their rate-limit keys.
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        format!("http://127.0.0.1:{}", addr.port())
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn http_server_reachable_true_against_live_server() {
        let pool = crate::db::open_memory().unwrap();
        let app = build_test_app(pool, "http://127.0.0.1");
        let base = serve_ephemeral(app).await;

        let probe = http_server_reachable(&test_client(), &base).await.unwrap();
        assert!(probe.reachable);
        assert_eq!(probe.status, Some(200));
    }

    #[tokio::test]
    async fn oauth_discovery_passes_against_live_server() {
        let pool = crate::db::open_memory().unwrap();
        let issuer = "http://127.0.0.1:9999";
        let app = build_test_app(pool, issuer);
        let base = serve_ephemeral(app).await;

        let c = check_oauth_discovery(&test_client(), &base).await;
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
        // The discovery `resource` is issuer + /mcp per oauth.rs.
        assert!(c.detail.contains("/mcp"), "detail: {}", c.detail);
    }

    #[tokio::test]
    async fn mcp_no_key_gets_401_with_www_authenticate() {
        let pool = crate::db::open_memory().unwrap();
        let app = build_test_app(pool, "http://127.0.0.1");
        let base = serve_ephemeral(app).await;

        let c = check_mcp(&test_client(), &base, None).await;
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
        assert!(c.detail.contains("auth enforced"), "detail: {}", c.detail);
    }

    #[tokio::test]
    async fn mcp_with_real_key_completes_initialize() {
        let pool = crate::db::open_memory().unwrap();
        let manager = crate::auth::create_key_manager().unwrap();
        let key = crate::auth::create_api_key(&pool, &manager, "doctor-test", None).unwrap();
        let app = build_test_app(pool, "http://127.0.0.1");
        let base = serve_ephemeral(app).await;

        let c = check_mcp(&test_client(), &base, Some(&key)).await;
        assert_eq!(c.status, Status::Pass, "detail: {}", c.detail);
        assert!(c.detail.contains("serverInfo"), "detail: {}", c.detail);
    }

    #[tokio::test]
    async fn mcp_with_wrong_key_fails() {
        let pool = crate::db::open_memory().unwrap();
        let app = build_test_app(pool, "http://127.0.0.1");
        let base = serve_ephemeral(app).await;

        // A syntactically plausible but nonexistent key.
        let bogus = "lific_sk-live-AAAAAAAAAAAAAAAAAAAAAAAAAAAA.0000000000000000";
        let c = check_mcp(&test_client(), &base, Some(bogus)).await;
        assert_eq!(c.status, Status::Fail, "detail: {}", c.detail);
    }

    // ── full report offline: no fails, server checks skipped ─────────────

    #[tokio::test]
    async fn full_report_offline_has_no_fails_and_skips_http() {
        // build_report reaches credentials::load_with_source, which reads
        // LIFIC_TOKEN from the process env. Other tests mutate that variable
        // with unsafe setenv; an unsynchronized getenv racing one of those
        // can observe a reallocated environ. Hold the crate-wide lock across
        // the whole report build (LIF-401).
        let _env = crate::test_env::lock_lific_token_env().await;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut cfg = Config::default();
        cfg.database.path = dir.join("lific.db");
        // Point at a port nothing listens on.
        cfg.server.host = "127.0.0.1".into();
        cfg.server.port = 1;
        cfg.backup.enabled = false;

        let report = build_report(&cfg, None).await;
        assert_eq!(report.fail_count(), 0, "offline run must not fail");
        assert!(report.ok);

        // server = warn, oauth_discovery + mcp = skipped
        let by = |n: &str| report.checks.iter().find(|c| c.name == n).map(|c| c.status);
        assert_eq!(by("server"), Some(Status::Warn));
        assert_eq!(by("oauth_discovery"), Some(Status::Skipped));
        assert_eq!(by("mcp"), Some(Status::Skipped));
    }
}
