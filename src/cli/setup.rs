//! Instance initialization and background-service lifecycle.

use super::{self as cli, ServiceAction};
use crate::config::{Config, local_url};
use crate::{auth, config, db, first_boot};

/// The three operations the in-place rewrite needs from an open config file.
/// A trait rather than `File` directly so a test can fail the write and prove
/// the rollback puts the original bytes back.
#[cfg(unix)]
trait ConfigSink {
    /// Write `bytes` starting at offset 0, leaving any trailing bytes alone.
    fn write_at_start(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn truncate(&mut self, len: u64) -> std::io::Result<()>;
    fn sync(&mut self) -> std::io::Result<()>;
}

#[cfg(unix)]
impl ConfigSink for std::fs::File {
    fn write_at_start(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        std::io::Seek::seek(self, std::io::SeekFrom::Start(0))?;
        std::io::Write::write_all(self, bytes)
    }

    fn truncate(&mut self, len: u64) -> std::io::Result<()> {
        self.set_len(len)
    }

    fn sync(&mut self) -> std::io::Result<()> {
        self.sync_all()
    }
}

/// Overwrite a config in place without ever passing through an empty file.
///
/// Truncate-then-write loses the old configuration outright if the write dies
/// half way (ENOSPC, EIO). Instead the new bytes go down over the old ones and
/// the file is shortened only once they are durable, so a failure leaves either
/// the new config or, after the rollback, the original one.
#[cfg(unix)]
fn overwrite_config_bytes(
    sink: &mut dyn ConfigSink,
    original: &[u8],
    contents: &[u8],
) -> std::io::Result<()> {
    match write_config_bytes(sink, original.len(), contents) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Every failure lands here, the shortening included: a truncate or
            // its sync can fail on its own (EIO, or a filesystem that only
            // discovers a quota problem at flush), and leaving the file as the
            // new config plus a tail of the old one is not a config at all.
            // Best effort: if the restore fails too the file is genuinely
            // damaged and there is nothing left to try, so the caller still
            // sees the original cause.
            let _ = restore_config_bytes(sink, original);
            Err(error)
        }
    }
}

/// The new bytes over the old ones, shortened only once they are durable.
#[cfg(unix)]
fn write_config_bytes(
    sink: &mut dyn ConfigSink,
    original_len: usize,
    contents: &[u8],
) -> std::io::Result<()> {
    sink.write_at_start(contents)?;
    ConfigSink::sync(sink)?;
    if contents.len() < original_len {
        sink.truncate(contents.len() as u64)?;
        ConfigSink::sync(sink)?;
    }
    Ok(())
}

/// Put the file back the way it was found.
#[cfg(unix)]
fn restore_config_bytes(sink: &mut dyn ConfigSink, original: &[u8]) -> std::io::Result<()> {
    sink.write_at_start(original)?;
    sink.truncate(original.len() as u64)?;
    ConfigSink::sync(sink)
}

/// Rewrite an existing config file through its own descriptor.
///
/// LIF-469: the fallback for a writable file inside an unwritable directory.
/// It is not crash-atomic (a crash between the write and the truncate can
/// leave trailing bytes of the old config), which is why it runs only when
/// staging a replacement is impossible.
///
/// Unix only. Without `O_NOFOLLOW` the fallback would follow a symlink planted
/// by whoever owns that directory, and truncating an attacker-chosen file is
/// not a trade worth making for a convenience path, so elsewhere the atomic
/// path is the only path.
#[cfg(unix)]
fn rewrite_config_in_place(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = std::fs::OpenOptions::new()
        // Read as well as write, so the original bytes come off the descriptor
        // we already hold rather than off the path a second time.
        .read(true)
        .write(true)
        .create(false)
        // Refuse to follow a symlink: the whole point of this path is that
        // something else owns the directory, so the name could be a trap
        // pointing at a file we should not be writing to.
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    // Best effort: the file may be owned by another uid, and chmod is not
    // what makes this write correct.
    match file.set_permissions(std::fs::Permissions::from_mode(0o600)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(error),
    }
    let mut original = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut original)?;
    overwrite_config_bytes(&mut file, &original, contents.as_bytes())
}

#[cfg(not(unix))]
fn rewrite_config_in_place(path: &std::path::Path, _contents: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "cannot rewrite {} in place: no symlink-safe open on this platform",
            path.display()
        ),
    ))
}

fn write_private_config(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let staging = match tempfile::Builder::new()
        .prefix(".lific-config-")
        .tempdir_in(parent)
    {
        Ok(staging) => staging,
        // LIF-469: a container can hand us a writable config file inside a
        // directory we may not create entries in (Fly injects
        // /etc/lific/lific.toml into a root-owned /etc/lific). Staging plus
        // rename is impossible there, but rewriting the existing file is not.
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                && path.symlink_metadata().is_ok() =>
        {
            return rewrite_config_in_place(path, contents);
        }
        Err(error) => return Err(error),
    };
    let temp = staging.path().join(path.file_name().unwrap_or_default());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&temp)?;
    std::io::Write::write_all(&mut file, contents.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.sync_all()?;
    std::fs::rename(temp, path)?;
    sync_parent_dir(parent)
}

/// Flush the directory entry that publishes a config file. The file's own
/// `sync_all` persists its bytes; the name that reaches them lives in the
/// parent directory and survives a crash only once that is synced too.
/// Unix only: Windows exposes no directory handle to sync.
#[cfg_attr(
    not(unix),
    expect(clippy::unnecessary_wraps, reason = "fallible on Unix")
)]
fn sync_parent_dir(_dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(_dir)?.sync_all()?;
    }
    Ok(())
}

fn create_private_config(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".lific-config-")
        .tempdir_in(parent)?;
    let temp = staging.path().join(path.file_name().unwrap_or_default());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&temp)?;
    std::io::Write::write_all(&mut file, contents.as_bytes())?;
    file.sync_all()?;
    // A hard link publishes only when the destination does not yet exist.
    // It is atomic and leaves an existing configuration untouched on races.
    std::fs::hard_link(&temp, path)?;
    sync_parent_dir(parent)
}

use tracing::info;

/// Poll `<base>/api/health` until it answers 200 or the deadline passes.
async fn wait_healthy(base_url: &str, timeout: std::time::Duration) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{base_url}/api/health");
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(resp) = client.get(&url).send().await
            && resp.status().is_success()
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    false
}

/// `lific init`: everything needed to go from nothing to a running, reachable
/// instance in one command — config, database, initial API key, and a
/// background service that survives reboot. Idempotent: re-running repairs
/// whatever is missing and never overwrites existing config or keys.
/// LIF-295: where `lific init` roots the instance.
///
/// Returns `(config_path, default_db_path)`; `default_db_path` is `Some`
/// only for the OS-dirs layout, where the generated config must carry an
/// explicit absolute `database.path` (config dir and data dir differ).
///
/// - `--config <p>` → root at `p`, relative db beside it.
/// - `--here`, or a `lific.toml` already in the cwd (repairing an existing
///   directory-local instance must win over silently starting a second
///   instance in the OS dirs), or unresolvable platform dirs → cwd layout.
/// - otherwise → OS config dir + OS data dir (`Config::os_default_instance`).
fn resolve_init_target(
    config_flag: Option<&std::path::Path>,
    here: bool,
    cwd_config_exists: bool,
    os_default: Option<(std::path::PathBuf, std::path::PathBuf)>,
) -> (std::path::PathBuf, Option<std::path::PathBuf>) {
    if let Some(p) = config_flag {
        return (p.to_path_buf(), None);
    }
    if here || cwd_config_exists {
        return (std::path::PathBuf::from("lific.toml"), None);
    }
    match os_default {
        Some((config, db)) => (config, Some(db)),
        None => (std::path::PathBuf::from("lific.toml"), None),
    }
}

/// The TOML `init` writes when it creates a brand-new config file.
///
/// LIF-432: an explicit `--db` has to survive into the file. It used to be
/// applied only to the in-memory config that `init` seeded the database
/// through, so `lific --db ./smoke.db init` created the admin in `smoke.db`
/// and then wrote a config naming `lific.db`. The next config-only command
/// created that second, empty database and reported no users, with both files
/// on disk and nothing saying they had diverged.
///
/// The path is absolutized first: `init` resolves a relative `--db` against
/// the process cwd, but [`Config::load`] anchors a relative `database.path` to
/// the config file's own directory. Under the OS-dirs layout those are two
/// different directories, so writing the relative form would reintroduce the
/// same divergence by a longer route.
fn init_config_toml(
    db_flag: Option<&std::path::Path>,
    default_db: Option<&std::path::Path>,
) -> String {
    match db_flag {
        Some(db) => Config::default_toml_with_db(&config::absolutize(db)),
        None => match default_db {
            Some(db) => Config::default_toml_with_db(db),
            None => Config::default_toml(),
        },
    }
}

/// Load the config file `init` operates on, applying the optional `--db`
/// override on top. Shared by the initial load and the post-auth-mode reload
/// (LIFIC-25), so the override logic lives in exactly one place. A malformed
/// config file is fatal, matching `Config::resolve`'s contract everywhere else.
fn load_config_for_init(
    config_path: &std::path::Path,
    db_flag: Option<&std::path::Path>,
) -> Result<Config, config::ConfigError> {
    let mut cfg = Config::resolve(Some(config_path))?.config;
    if let Some(db) = db_flag {
        cfg.database.path = db.to_path_buf();
    }
    Ok(cfg)
}

/// Resolve the auth mode the operator chose at `init` (LIFIC-25). Honors an
/// explicit `--auth-mode` flag (non-interactive); otherwise, on a TTY, shows
/// the interactive menu. Refuses (rather than hangs) off a TTY, matching
/// `prompt_text`/`confirm`, and names the bypass flag.
fn resolve_auth_mode(
    flag: &Option<String>,
) -> Result<config::AuthMode, Box<dyn std::error::Error>> {
    if let Some(value) = flag {
        return config::AuthMode::parse(value).ok_or_else(|| {
            format!("invalid --auth-mode '{value}': expected login-free or passwords").into()
        });
    }
    if !cli::term::stdin_is_tty() {
        return Err(
            "auth-mode selection requires a terminal; re-run with --auth-mode login-free|passwords"
                .into(),
        );
    }
    let mut prompt = cliclack::Select::new("How do you want to sign in?");
    prompt = prompt
        .item(
            config::AuthMode::LoginFree,
            "Login-free",
            "no password; your browser signs you in; binds to 127.0.0.1",
        )
        .item(
            config::AuthMode::Passwords,
            "Passwords",
            "set a password and sign in on the web",
        );
    let mode = prompt
        .interact()
        .map_err(|e| -> Box<dyn std::error::Error> {
            if e.kind() == std::io::ErrorKind::Interrupted {
                "cancelled".into()
            } else {
                format!("auth-mode selection failed: {e}").into()
            }
        })?;
    if mode == config::AuthMode::LoginFree
        && !cli::term::confirm(
            &format!("{}\n\nProceed?", config::login_free_caution()),
            "--auth-mode login-free",
        )?
    {
        return Err("cancelled".into());
    }
    Ok(mode)
}

/// Prompt for the operator's password in `--auth-mode passwords`. Masked on a
/// TTY; read-a-line when piped (so scripts can supply it), matching the `user
/// create` flow.
fn prompt_password_for_auth_mode() -> Result<String, Box<dyn std::error::Error>> {
    if cli::term::stdin_is_tty() {
        Ok(cliclack::password("Operator password").interact()?)
    } else {
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        Ok(buf.trim().to_string())
    }
}

// clap can't express the --config conflict, and init threads many small flags;
// the repo tolerates this for command handlers (see cli/import.rs).
#[allow(clippy::too_many_arguments)]
pub(super) async fn cmd_init(
    config_flag: Option<&std::path::Path>,
    db_flag: Option<&std::path::Path>,
    json_flag: bool,
    no_service: bool,
    here: bool,
    name: Option<String>,
    auth_mode_flag: Option<String>,
    password_flag: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use cli::ui;
    // clap can't express this conflict: --config is a global arg on the
    // top-level Cli, out of the subcommand's conflicts_with reach.
    if here && config_flag.is_some() {
        return Err("--here conflicts with --config — pick one location".into());
    }
    let json = cli::term::wants_json(json_flag);
    if !json {
        ui::intro("lific init");
    }
    // LIF-292 + LIF-295: the instance roots wherever the config file lives —
    // an explicit --config, the cwd (--here / existing ./lific.toml), or the
    // OS-standard config+data dirs by default.
    let (config_path, default_db) = resolve_init_target(
        config_flag,
        here,
        std::path::Path::new("lific.toml").exists(),
        Config::os_default_instance(),
    );
    let created_config = if config_path.exists() {
        false
    } else {
        if let Some(parent) = config_path.parent()
            && !parent.as_os_str().is_empty()
        {
            let parent_existed = parent.exists();
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if !parent_existed {
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
                }
            }
            #[cfg(not(unix))]
            let _ = parent_existed;
        }
        let toml = init_config_toml(db_flag, default_db.as_deref());
        create_private_config(&config_path, &toml)?;
        true
    };

    // LIF-432, the other half: an existing config keeps its own database path,
    // so `--db` redirects only this run. Seeding one database while every
    // later command reads another is the exact failure this issue described,
    // and it is worth a word even when we cannot fix it by rewriting the file.
    if !created_config
        && let Some(db) = db_flag
        && let Ok(resolved) = Config::resolve(Some(&config_path))
        && config::absolutize(&resolved.config.database.path) != config::absolutize(db)
    {
        let msg = format!(
            "--db points at {} but {} says {}. init will seed the --db path; later commands \
             reading only the config will use the other one.",
            config::absolutize(db).display(),
            config_path.display(),
            resolved.config.database.path.display()
        );
        if json {
            eprintln!("warning: {msg}");
        } else {
            ui::warn(msg);
        }
    };

    // (Re)load from the file init actually operates on, so a relative
    // database.path anchors to the config's own directory — the same
    // resolution the installed service (WorkingDirectory = that directory)
    // applies at runtime. The pre-dispatch Config::resolve can't have done
    // this when the file didn't exist yet. Applied again after the auth-mode
    // edit rewrites the file (LIFIC-25).
    let mut cfg = load_config_for_init(&config_path, db_flag)?;

    // Create + migrate the database and seed instance settings now, while the
    // instance has zero users — this is the moment the authz-enforced default
    // is decided. The data dir may not exist yet under the OS-dirs layout
    // (LIF-295: db lives in ~/.local/share/lific/, not beside the config).
    if let Some(parent) = cfg.database.path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let pool = db::open(&cfg.database.path)?;
    {
        let conn = pool.write()?;
        db::queries::settings::ensure(&conn, cfg.auth.allow_signup)?;
    }

    // LIFIC-25: on a fresh install (no human operator yet) the operator picks
    // an auth mode — login-free or passwords. Resolve it (flag, or an
    // interactive TTY menu), persist the choice to the config file + database,
    // and create the first admin in that mode. An existing instance with users
    // skips all of this entirely.
    let created_admin = if !auth::has_human_operator(&pool) {
        let mode = resolve_auth_mode(&auth_mode_flag)?;

        // Persist the choice into the config file, editing it in place (the
        // change set `[auth] required` and `[server] host`; every other section
        // and setting survives). Reload cfg so downstream (local_url, JSON,
        // service plan) reflects required/host.
        let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
        let new_toml = Config::apply_auth_mode(&existing, mode.required(), mode.host())?;
        write_private_config(&config_path, &new_toml)?;
        cfg = load_config_for_init(&config_path, db_flag)?;

        let op_name = match name {
            Some(n) => n,
            None => cli::term::prompt_text("What's your name?", "--name")
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?,
        };

        // Write web_auto_login to the DB beside the admin (it lives in the
        // database, not the config). On for login-free so the browser signs the
        // operator in; off for password mode.
        let conn = pool.write()?;
        let password = if mode.passwordless() {
            None
        } else {
            Some(match &password_flag {
                Some(p) => p.clone(),
                None => prompt_password_for_auth_mode()?,
            })
        };
        // Shared with `start --init-if-missing` (LIF-468) so both first-run
        // paths agree on what a fresh instance looks like.
        let admin = first_boot::create_first_admin(
            &conn,
            &op_name,
            password.as_deref(),
            mode.web_auto_login(),
        )?;
        info!(operator = %admin.username, mode = mode.as_str(), "created first human admin");
        Some(admin)
    } else {
        None
    };

    // Mint the initial API key HERE, in the operator's terminal. Once the
    // server runs as a background service, its stdout goes to the journal
    // where nobody would see a printed key. LIFIC-9: once a human admin exists
    // we stop auto-minting the unbound "default" key — the operator is a real
    // user now, and keys are minted on demand via `lific key create`.
    let new_key = if auth::should_mint_initial_key(&pool) {
        let manager =
            auth::create_key_manager().map_err(|e| format!("key manager init failed: {e}"))?;
        Some(auth::create_api_key(&pool, &manager, "default", None)?)
    } else {
        None
    };
    // Release the CLI's DB handles before the service process opens the file.
    drop(pool);

    // Background service: the README's 60-second setup has to end with a
    // server that is still alive tomorrow, not a process tied to a terminal.
    let url = local_url(&cfg);
    let mut service_report = None;
    let mut service_error = None;
    let mut healthy = false;
    if !no_service {
        match cli::service::detect() {
            Some(mgr) => {
                let plan = cli::service::ServicePlan::for_config_file(&config_path)?;
                match cli::service::install(mgr, &plan) {
                    Ok(report) => {
                        healthy = wait_healthy(&url, std::time::Duration::from_secs(15)).await;
                        // A 200 alone can lie (another process may own the
                        // port while our unit crash-loops on AddrInUse), and
                        // silence alone is ambiguous. Cross-check the unit's
                        // own active state to say something precise.
                        let active = cli::service::status(mgr).is_ok_and(|s| s.active);
                        match (healthy, active) {
                            (true, true) => {}
                            (true, false) => {
                                healthy = false;
                                service_error = Some(format!(
                                    "something is answering at {url}, but it isn't the \
                                     installed service — another server is likely already \
                                     using the port. Check: {}",
                                    cli::service::logs_hint(mgr)
                                ));
                            }
                            (false, false) => {
                                service_error = Some(format!(
                                    "the service failed to stay running — most often the \
                                     port is already in use. Check: {}",
                                    cli::service::logs_hint(mgr)
                                ));
                            }
                            (false, true) => {
                                service_error = Some(format!(
                                    "the service is running but didn't answer at {url} \
                                     within 15s. Check: {}",
                                    cli::service::logs_hint(mgr)
                                ));
                            }
                        }
                        service_report = Some((mgr, report));
                    }
                    Err(e) => service_error = Some(e),
                }
            }
            None => {
                service_error = Some(
                    "no supported service manager found (needs a systemd user session on \
                     Linux, or launchd on macOS)"
                        .to_string(),
                )
            }
        }
    }

    if json {
        let out = serde_json::json!({
            "config": { "path": config_path.display().to_string(), "created": created_config },
            "database": cfg.database.path.display().to_string(),
            "key": new_key,
            "admin": created_admin.as_ref().map(|a| serde_json::json!({
                "id": a.id,
                "username": a.username,
                "display_name": a.display_name,
                "is_admin": a.is_admin,
            })),
            "url": url,
            "service": {
                "requested": !no_service,
                "installed": service_report.as_ref().map(|(_, r)| serde_json::to_value(r).unwrap_or_default()),
                "healthy": healthy,
                "error": service_error,
            },
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    if created_config {
        ui::step(format!("Created {}", config_path.display()));
    } else {
        ui::step(format!("Using existing {}", config_path.display()));
    }
    ui::step(format!(
        "Database ready {}",
        ui::dim(cfg.database.path.display())
    ));

    if let Some(ref admin) = created_admin {
        ui::step(format!(
            "First operator {} created — passwordless mode is on",
            ui::command(&admin.display_name)
        ));
    }

    if let Some(ref key) = new_key {
        ui::note(
            "Initial API key — save it now, it will not be shown again",
            format!("{key}\n\nUse it as: Authorization: Bearer <key>"),
        );
    }

    if let Some((mgr, ref report)) = service_report {
        ui::step(format!(
            "Service installed — {} {}",
            report.manager,
            ui::dim(&report.definition)
        ));
        if report.linger == Some(false) {
            ui::warn(
                "`loginctl enable-linger` didn't succeed — the service will stop when you \
                 log out. Run it manually to fix that.",
            );
        }
        if healthy {
            ui::step(format!("Lific is running at {}", ui::command(&url)));
        } else if let Some(ref e) = service_error {
            ui::warn(e);
        } else {
            ui::warn(format!(
                "service started but the server didn't answer at {url} within 15s — check \
                 logs: {}",
                cli::service::logs_hint(mgr)
            ));
        }
    } else if no_service {
        ui::info(format!(
            "Service install skipped (--no-service). Run the server with {}",
            ui::command("lific start")
        ));
    } else if let Some(e) = service_error {
        ui::warn(format!("couldn't install a background service: {e}"));
        ui::info(format!(
            "run the server in the foreground instead: {}",
            ui::command("lific start")
        ));
    }

    ui::note(
        "Next steps",
        format!(
            "1. Open {url} and create your account\n2. {}\n3. {}   {}",
            ui::command("lific user promote --username <you>"),
            ui::command("lific connect"),
            ui::dim("# wire up your AI tools"),
        ),
    );

    let mut outro_msg = format!("Verify anytime with {}", ui::command("lific doctor"));
    if service_report.is_some() {
        outro_msg.push_str(&format!(
            " · manage the service with {}",
            ui::command("lific service status|restart|stop|uninstall")
        ));
    }
    ui::outro(outro_msg);
    Ok(())
}

/// `lific service <action>`: manage the background service `init` installs.
pub(super) fn cmd_service(
    cfg: &Config,
    resolved_config_path: Option<&std::path::Path>,
    json_flag: bool,
    action: &ServiceAction,
) -> Result<(), Box<dyn std::error::Error>> {
    use cli::ui;
    let json = cli::term::wants_json(json_flag);
    let Some(mgr) = cli::service::detect() else {
        return Err(
            "no supported service manager found (needs a systemd user session on \
                    Linux, or launchd on macOS)"
                .into(),
        );
    };
    match action {
        ServiceAction::Install => {
            let config_path = match resolved_config_path {
                Some(path) => path,
                None => {
                    let (init_path, _) = resolve_init_target(
                        None,
                        false,
                        std::path::Path::new("lific.toml").exists(),
                        Config::os_default_instance(),
                    );
                    return Err(format!(
                        "no configuration file selected — run `lific init` to create '{}' or pass --config PATH",
                        init_path.display()
                    )
                    .into());
                }
            };
            let plan = cli::service::ServicePlan::for_config_file(config_path)?;
            let report = cli::service::install(mgr, &plan)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                ui::intro("lific service install");
                ui::step(format!(
                    "Service installed and started — {} {}",
                    report.manager,
                    ui::dim(&report.definition)
                ));
                if report.linger == Some(false) {
                    ui::warn(
                        "`loginctl enable-linger` didn't succeed — the service will stop \
                         when you log out. Run it manually to fix that.",
                    );
                }
                ui::outro(format!(
                    "Logs: {}",
                    ui::command(cli::service::logs_hint(mgr))
                ));
            }
        }
        ServiceAction::Uninstall => {
            let removed = cli::service::uninstall(mgr)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "uninstalled": true, "definition": removed })
                );
            } else {
                ui::intro("lific service uninstall");
                ui::step(format!(
                    "Service stopped and uninstalled {}",
                    ui::dim(&removed)
                ));
                ui::outro(format!(
                    "Reinstall anytime with {}",
                    ui::command("lific service install")
                ));
            }
        }
        ServiceAction::Status => {
            let s = cli::service::status(mgr)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else if s.active {
                ui::step(format!(
                    "Service is running ({}) — {}",
                    s.manager,
                    ui::command(local_url(cfg))
                ));
            } else if s.installed {
                ui::error(format!(
                    "Service is installed but NOT running ({}). Start it: {}",
                    s.manager,
                    ui::command("lific service restart")
                ));
            } else {
                ui::error(format!(
                    "Service is not installed. Install it: {}",
                    ui::command("lific service install")
                ));
            }
            if !(s.installed && s.active) {
                std::process::exit(1);
            }
        }
        ServiceAction::Stop => {
            cli::service::stop(mgr)?;
            if json {
                println!("{}", serde_json::json!({ "stopped": true }));
            } else {
                ui::step(format!(
                    "Service stopped {}",
                    ui::dim("(still installed; it returns on reboot or `lific service restart`)")
                ));
            }
        }
        ServiceAction::Restart => {
            cli::service::restart(mgr)?;
            if json {
                println!("{}", serde_json::json!({ "restarted": true }));
            } else {
                ui::step(format!(
                    "Service restarted — {}",
                    ui::command(local_url(cfg))
                ));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod init_target_tests {
    use super::{Config, auth, cmd_init, init_config_toml, resolve_init_target};
    use crate::db;
    use std::path::{Path, PathBuf};

    /// LIF-432: whatever database `init` actually seeds must be the database
    /// the config it writes names, or every later config-only command lands in
    /// a different, empty file without a word of warning.
    mod written_config_names_the_seeded_database {
        use super::*;

        fn db_path_in(toml: &str) -> String {
            toml.parse::<toml::Table>()
                .expect("init writes valid TOML")
                .get("database")
                .and_then(|d| d.get("path"))
                .and_then(|p| p.as_str())
                .expect("a [database] path is always written")
                .to_string()
        }

        #[test]
        fn an_explicit_db_flag_wins_over_the_os_default() {
            let toml = init_config_toml(
                Some(Path::new("/tmp/smoke.db")),
                Some(Path::new("/home/u/.local/share/lific/lific.db")),
            );
            assert_eq!(
                db_path_in(&toml),
                crate::config::absolutize(Path::new("/tmp/smoke.db"))
                    .display()
                    .to_string(),
                "--db is the path init seeds, so it is the path the config must name"
            );
        }

        #[test]
        fn a_relative_db_flag_is_written_absolute() {
            // init resolves a relative --db against the cwd; Config::load
            // anchors a relative database.path to the config file's directory.
            // Writing the relative form would let those two disagree whenever
            // the config does not live in the cwd, which is the default layout.
            let toml = init_config_toml(Some(Path::new("smoke.db")), None);
            let written = db_path_in(&toml);
            assert!(
                Path::new(&written).is_absolute(),
                "expected an absolute path, got {written}"
            );
            assert!(written.ends_with("smoke.db"));
        }

        #[test]
        fn without_a_db_flag_the_os_default_is_written_unchanged() {
            let toml = init_config_toml(None, Some(Path::new(super::OS_DEFAULT_DB_FIXTURE)));
            assert_eq!(db_path_in(&toml), super::OS_DEFAULT_DB_FIXTURE);
        }

        #[test]
        fn the_cwd_layout_keeps_the_plain_relative_default() {
            // --here / --config put the database beside the config file, where
            // a relative path is correct and anchoring resolves it properly.
            let toml = init_config_toml(None, None);
            assert_eq!(db_path_in(&toml), "lific.db");
        }
    }

    /// An absolute OS-data-dir path literal for the host platform.
    #[cfg(unix)]
    const OS_DEFAULT_DB_FIXTURE: &str = "/home/u/.local/share/lific/lific.db";
    #[cfg(not(unix))]
    const OS_DEFAULT_DB_FIXTURE: &str = "C:/Users/u/AppData/Roaming/lific/lific.db";

    fn os_default() -> (PathBuf, PathBuf) {
        (
            PathBuf::from("/home/u/.config/lific/lific.toml"),
            PathBuf::from("/home/u/.local/share/lific/lific.db"),
        )
    }

    // LIF-295: bare init targets the OS dirs, with an explicit db path so the
    // generated config can split config dir from data dir.
    #[test]
    fn bare_init_targets_os_dirs() {
        let (config, db) = resolve_init_target(None, false, false, Some(os_default()));
        assert_eq!(config, Path::new("/home/u/.config/lific/lific.toml"));
        assert_eq!(
            db.as_deref(),
            Some(Path::new("/home/u/.local/share/lific/lific.db"))
        );
    }

    #[test]
    fn here_flag_forces_cwd_layout() {
        let (config, db) = resolve_init_target(None, true, false, Some(os_default()));
        assert_eq!(config, Path::new("lific.toml"));
        assert_eq!(db, None, "cwd layout keeps the relative default db");
    }

    // Repairing an existing directory-local instance must win over creating
    // a second instance in the OS dirs.
    #[test]
    fn existing_cwd_config_wins_over_os_dirs() {
        let (config, db) = resolve_init_target(None, false, true, Some(os_default()));
        assert_eq!(config, Path::new("lific.toml"));
        assert_eq!(db, None);
    }

    #[test]
    fn explicit_config_flag_wins_over_everything() {
        let (config, db) = resolve_init_target(
            Some(Path::new("/srv/lific/lific.toml")),
            false,
            true,
            Some(os_default()),
        );
        assert_eq!(config, Path::new("/srv/lific/lific.toml"));
        assert_eq!(db, None);
    }

    #[test]
    fn unresolvable_platform_dirs_fall_back_to_cwd() {
        let (config, db) = resolve_init_target(None, false, false, None);
        assert_eq!(config, Path::new("lific.toml"));
        assert_eq!(db, None);
    }

    // The --here / --config conflict is enforced in cmd_init (clap can't
    // express it: --config is a global arg). The guard runs before any
    // filesystem access, so calling it here is side-effect free.
    #[tokio::test]
    async fn init_rejects_here_with_config() {
        let err = cmd_init(
            Some(Path::new("/tmp/nonexistent/lific.toml")),
            None,
            true, // json
            true, // no_service
            true, // here
            Some("test".into()),
            None, // auth_mode
            None, // password
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("--here conflicts with --config"));
    }

    // A temp dir that self-destructs, so cmd_init's filesystem writes stay out
    // of the repo tree and don't collide across tests.
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    /// Run `lific init --config <dir>/lific.toml --no-service` for the operator
    /// `name` and assert on the DB state it wrote (stdout isn't a TTY under the
    /// test harness, so we can't capture cmd_init's printed JSON — instead we
    /// re-open the database and read back the shared facts).
    async fn run_init(
        dir: &TempDir,
        name: Option<&str>,
        auth_mode: Option<&str>,
        password: Option<&str>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let config_path = dir.path().join("lific.toml");
        cmd_init(
            Some(&config_path),
            None,
            true,  // json
            true,  // no_service
            false, // here
            name.map(str::to_string),
            auth_mode.map(str::to_string),
            password.map(str::to_string),
        )
        .await?;
        let cfg = Config::load(Some(&config_path))?;
        let pool = db::open(&cfg.database.path)?;
        let conn = pool.read().unwrap();
        let admin = crate::db::queries::users::first_admin(&conn)?;
        let settings = crate::db::queries::settings::get(&conn).ok();
        Ok(serde_json::json!({
            "admin": admin.as_ref().map(|a| a.username.clone()),
            "admin_display": admin.as_ref().map(|a| a.display_name.clone()),
            "keys": auth::has_any_keys(&pool),
            "host": cfg.server.host,
            "required": cfg.auth.required,
            "web_auto_login": settings.map(|s| s.web_auto_login),
        }))
    }

    // LIFIC-9: a fresh install (no humans) creates the first passwordless admin
    // when given `--name` non-interactively (login-free mode).
    #[tokio::test]
    async fn init_fresh_install_creates_first_admin_with_name() {
        let dir = temp_dir();
        let out = run_init(&dir, Some("Blake Alston"), Some("login-free"), None)
            .await
            .unwrap();
        assert_eq!(out["admin"], serde_json::json!("blake-alston"));
    }

    // LIFIC-9: once a human admin exists, init skips minting the unbound
    // "default" key (passwordless mode) — no key is auto-generated.
    #[tokio::test]
    async fn init_fresh_install_skips_default_key_when_admin_created() {
        let dir = temp_dir();
        let out = run_init(&dir, Some("Blake"), Some("login-free"), None)
            .await
            .unwrap();
        assert_eq!(out["admin"], serde_json::json!("blake"));
        assert_eq!(
            out["keys"],
            serde_json::json!(false),
            "a human operator exists, so no unbound default key is minted"
        );
    }

    // LIFIC-9: re-running init on an existing instance (admins already exist)
    // skips creation — idempotent, existing setup untouched.
    #[tokio::test]
    async fn init_existing_install_skips_admin_creation() {
        let dir = temp_dir();
        let first = run_init(&dir, Some("Blake"), Some("login-free"), None)
            .await
            .unwrap();
        assert_eq!(first["admin"], serde_json::json!("blake"));

        // Second run with a different name must NOT create a second admin.
        let second = run_init(
            &dir,
            Some("Someone Else"),
            Some("passwords"),
            Some("hunter22!"),
        )
        .await
        .unwrap();
        assert_eq!(
            second["admin"],
            serde_json::json!("blake"),
            "existing instance keeps its first admin"
        );
    }

    // LIFIC-25: login-free mode writes required=false, host=127.0.0.1,
    // web_auto_login=true, and a passwordless admin.
    #[tokio::test]
    async fn init_login_free_wires_config_db_and_passwordless_admin() {
        let dir = temp_dir();
        let out = run_init(&dir, Some("Blake"), Some("login-free"), None)
            .await
            .unwrap();
        assert_eq!(out["admin_display"], serde_json::json!("Blake"));
        assert_eq!(out["required"], serde_json::json!(false));
        assert_eq!(out["host"], serde_json::json!("127.0.0.1"));
        assert_eq!(out["web_auto_login"], serde_json::json!(true));
    }

    // LIFIC-25: password mode writes required=true, leaves host unchanged,
    // web_auto_login=false, and creates an admin with the chosen password.
    #[tokio::test]
    async fn init_passwords_wires_config_db_and_passworded_admin() {
        let dir = temp_dir();
        let out = run_init(&dir, Some("Blake"), Some("passwords"), Some("hunter22!"))
            .await
            .unwrap();
        assert_eq!(out["required"], serde_json::json!(true));
        // host is left at its default (0.0.0.0) — password mode never binds loopback.
        assert_eq!(out["host"], serde_json::json!("0.0.0.0"));
        assert_eq!(out["web_auto_login"], serde_json::json!(false));
        // Passworded admin can sign in.
        assert_eq!(out["admin"], serde_json::json!("blake"));
    }

    // LIFIC-25: an invalid --auth-mode is rejected.
    #[tokio::test]
    async fn init_rejects_invalid_auth_mode() {
        let dir = temp_dir();
        let config_path = dir.path().join("lific.toml");
        let err = cmd_init(
            Some(&config_path),
            None,
            true, // json
            true, // no_service
            false,
            Some("Blake".to_string()),
            Some("bogus".to_string()),
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid --auth-mode"));
    }
}

/// LIF-469: `init --force` rewrites the config in place when the directory
/// holding it refuses new entries, which is the shape a container gives us
/// when it injects a config file into a root-owned directory.
#[cfg(all(test, unix))]
mod write_private_config_tests {
    use super::{ConfigSink, overwrite_config_bytes, write_private_config};
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    /// Restores the directory's mode on drop, including on a failed assert, so
    /// the `TempDir` can still delete itself.
    struct ModeGuard {
        dir: PathBuf,
        mode: u32,
    }

    impl ModeGuard {
        fn seal(dir: &Path) -> Self {
            let mode = fs::metadata(dir)
                .expect("temp dir exists")
                .permissions()
                .mode();
            let guard = Self {
                dir: dir.to_path_buf(),
                mode,
            };
            fs::set_permissions(dir, fs::Permissions::from_mode(0o500))
                .expect("dropping write on a temp dir we own");
            guard
        }
    }

    impl Drop for ModeGuard {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(self.mode));
        }
    }

    /// Root ignores the directory's write bit, so the fallback never triggers.
    fn skip_as_root() -> bool {
        // SAFETY: geteuid is always safe; it reads a process attribute.
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn an_unwritable_directory_still_rewrites_a_writable_config() {
        if skip_as_root() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("lific.toml");
        fs::write(&config, "old = true\n").expect("seed config");
        let _guard = ModeGuard::seal(dir.path());

        write_private_config(&config, "new = true\n").expect("a writable file is rewritable");

        assert_eq!(
            fs::read_to_string(&config).expect("read back"),
            "new = true\n"
        );
        let strays: Vec<_> = fs::read_dir(dir.path())
            .expect("list dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".lific-config-"))
            .collect();
        assert!(
            strays.is_empty(),
            "no staging left behind, found {strays:?}"
        );
    }

    #[test]
    fn the_in_place_fallback_refuses_a_symlinked_config() {
        if skip_as_root() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let target_dir = tempfile::tempdir().expect("temp dir");
        let target = target_dir.path().join("real.toml");
        fs::write(&target, "secret = true\n").expect("seed target");
        let config = dir.path().join("lific.toml");
        std::os::unix::fs::symlink(&target, &config).expect("symlink");
        let _guard = ModeGuard::seal(dir.path());

        let error = write_private_config(&config, "new = true\n")
            .expect_err("a symlink is not a config we may truncate");

        assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(
            fs::read_to_string(&target).expect("read target"),
            "secret = true\n",
            "the symlink target must be untouched"
        );
    }

    #[test]
    fn a_missing_config_in_an_unwritable_directory_reports_the_original_error() {
        if skip_as_root() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("lific.toml");
        let _guard = ModeGuard::seal(dir.path());

        let error =
            write_private_config(&config, "new = true\n").expect_err("nothing to rewrite here");

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    /// Stands in for a file whose write dies part way through: ENOSPC, EIO,
    /// a full quota. The first write lands `accept` bytes and then fails.
    struct FailingSink {
        bytes: Vec<u8>,
        writes: usize,
        fail_first_write_after: Option<usize>,
        /// Fail the next `truncate` and only that one, so the rollback's own
        /// truncate can still succeed and the restored bytes are observable.
        fail_next_truncate: bool,
    }

    impl ConfigSink for FailingSink {
        fn write_at_start(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            self.writes += 1;
            let accept = match self.fail_first_write_after.take() {
                Some(accept) => accept.min(bytes.len()),
                None => bytes.len(),
            };
            if self.bytes.len() < accept {
                self.bytes.resize(accept, 0);
            }
            self.bytes[..accept].copy_from_slice(&bytes[..accept]);
            if accept < bytes.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::StorageFull,
                    "no space left on device",
                ));
            }
            Ok(())
        }

        fn truncate(&mut self, len: u64) -> std::io::Result<()> {
            if std::mem::take(&mut self.fail_next_truncate) {
                return Err(std::io::Error::other("truncate failed"));
            }
            self.bytes.resize(len as usize, 0);
            Ok(())
        }

        fn sync(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_failed_write_restores_the_original_config() {
        let original = b"database.path = \"/data/lific.db\"\n".to_vec();
        let mut sink = FailingSink {
            bytes: original.clone(),
            writes: 0,
            fail_first_write_after: Some(6),
            fail_next_truncate: false,
        };

        let error = overwrite_config_bytes(&mut sink, &original, b"replacement config\n")
            .expect_err("the sink fails mid-write");

        assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
        assert_eq!(
            sink.bytes, original,
            "a half-written config must be rolled back, not left in place"
        );
        assert_eq!(sink.writes, 2, "one attempt, one rollback");
    }

    #[test]
    fn a_shorter_config_is_truncated_only_after_the_new_bytes_are_durable() {
        let original = b"a-long-previous-configuration\n".to_vec();
        let mut sink = FailingSink {
            bytes: original.clone(),
            writes: 0,
            fail_first_write_after: None,
            fail_next_truncate: false,
        };

        overwrite_config_bytes(&mut sink, &original, b"short\n").expect("a clean write");

        assert_eq!(
            sink.bytes, b"short\n",
            "no trailing bytes of the old config may survive"
        );
    }

    /// The shortening is part of the write, not an afterthought: a failed
    /// truncate (or its sync) leaves the new config with a tail of the old one
    /// glued on, which is not a config at all. It rolls back like any other
    /// failure.
    #[test]
    fn a_failed_truncate_also_restores_the_original_config() {
        let original = b"a-long-previous-configuration\n".to_vec();
        let mut sink = FailingSink {
            bytes: original.clone(),
            writes: 0,
            fail_first_write_after: None,
            fail_next_truncate: true,
        };

        let error = overwrite_config_bytes(&mut sink, &original, b"short\n")
            .expect_err("the sink fails on truncate");

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert_eq!(
            sink.bytes, original,
            "a config left as new bytes plus an old tail must be rolled back"
        );
        assert_eq!(sink.writes, 2, "one attempt, one rollback");
    }

    #[test]
    fn a_shorter_config_leaves_no_trailing_bytes_on_disk() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("lific.toml");
        fs::write(&config, "old = true\nwith = \"a lot more text\"\n").expect("seed config");
        let _guard = ModeGuard::seal(dir.path());

        write_private_config(&config, "new = 1\n").expect("a writable file is rewritable");

        assert_eq!(fs::read_to_string(&config).expect("read back"), "new = 1\n");
    }

    #[test]
    fn a_writable_directory_still_publishes_through_a_staged_rename() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("lific.toml");
        fs::write(&config, "old = true\n").expect("seed config");
        let before = fs::metadata(&config).expect("stat").ino();

        write_private_config(&config, "new = true\n").expect("the normal path");

        let after = fs::metadata(&config).expect("stat").ino();
        assert_ne!(
            before, after,
            "a rename swaps in a new inode; an equal one means the atomic path was skipped"
        );
        assert_eq!(
            fs::read_to_string(&config).expect("read back"),
            "new = true\n"
        );
        assert_eq!(
            fs::metadata(&config).expect("stat").permissions().mode() & 0o777,
            0o600
        );
    }
}
