//! Parse once, select configuration, dispatch a command, then write its result.

use crate::{authz, repo_identity, resolve_caller};
use rmcp::ServiceExt;
use tracing::info;

use super::{self as cli, BackendKind, Cli, Command, setup};
use crate::config::{Config, local_url};
use crate::{
    actor, auth, config, db, dump, first_boot, links, mcp, project_archive, server, storage,
};
use clap::{CommandFactory, FromArgMatches};

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Via `ArgMatches` rather than `Cli::parse()` so a value's source stays
    // answerable: `lific mcp --instances` rejects a typed `--url` but ignores
    // an exported `LIFIC_URL`. Behaviour is otherwise identical.
    let matches = Cli::command().get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };

    // Rust ignores SIGPIPE process-wide, which makes println!/stdout writes
    // PANIC when piped into a closed reader (`lific completion fish | head`,
    // `lific issue list --json | head -1`). For data commands, restore the
    // default SIGPIPE disposition so the process exits quietly like every
    // other Unix CLI. The long-running servers (Start, Mcp) keep SIGPIPE
    // ignored — tokio socket writes rely on that to surface EPIPE as errors
    // instead of killing the process.
    #[cfg(unix)]
    if !matches!(cli.command, Command::Start { .. } | Command::Mcp { .. }) {
        // SAFETY: setting a signal disposition to SIG_DFL before any threads
        // depend on the ignored state; standard practice for CLI tools.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }

    // Shell completions must work with no lific.toml present and touch no DB,
    // so handle them before loading config or opening the database.
    if let Command::Completion { shell } = cli.command {
        clap_complete::generate(shell, &mut Cli::command(), "lific", &mut std::io::stdout());
        return Ok(());
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(dispatch(cli, matches))
}

async fn dispatch(cli: Cli, matches: clap::ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve config once. Normal commands fail closed on a selected config
    // error; doctor receives the same typed result and reports the failure
    // while continuing independent diagnostics.
    let resolution = Config::resolve(cli.config.as_deref());
    if let Command::Doctor { key, repair } = &cli.command {
        let json = cli::term::wants_json(cli.json);
        cli::doctor::run(resolution, cli.db.as_deref(), key.as_deref(), *repair, json)
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
        return Ok(());
    }
    let resolved_config_path = resolution
        .as_ref()
        .ok()
        .and_then(|resolved| resolved.path.clone());
    // Provenance, kept for `start --init-if-missing` (LIF-468): the built-in
    // default is the one source that may not be initialized from.
    let resolved_config_source = resolution
        .as_ref()
        .ok()
        .map_or(config::ConfigSource::BuiltInDefault, |resolved| {
            resolved.source
        });
    let mut cfg = match resolution {
        Ok(resolved) => resolved.config,
        Err(config::ConfigError::MissingExplicit { .. })
            if matches!(&cli.command, Command::Init { .. }) =>
        {
            Config::default()
        }
        Err(error) => return Err(error.into()),
    };

    // CLI overrides
    if let Some(ref db) = cli.db {
        cfg.database.path = db.clone();
    }

    if cli.backend == BackendKind::Http && !matches!(cli.command, Command::Data(_)) {
        return Err(
            "the HTTP backend currently supports data commands: issue, project, page, export, search, comment, module, label, folder, bind, and git-hook"
                .into(),
        );
    }
    if cli.backend == BackendKind::Sql && cli.command.needs_existing_database() {
        cfg.require_existing_database()?;
    }

    match cli.command {
        Command::Data(command) => {
            let json = cli::term::wants_json(cli.json);
            let output = match cli.backend {
                BackendKind::Http => {
                    let url = http_backend_url(
                        cli.url.as_deref(),
                        cfg.server.public_url.as_deref(),
                        &cfg,
                    );
                    let key = cli::resolve_http_credential(cli.api_key.as_deref(), || {
                        cli::credentials::load(&url)
                    })?;
                    cli::http::run(&command, &url, key.as_deref(), json).await?
                }
                BackendKind::Sql => {
                    actor::set_default_transport(actor::Transport::Cli);
                    let pool = db::open(&cfg.database.path)?;
                    let links = cfg
                        .server
                        .public_url
                        .as_deref()
                        .and_then(links::IssueLinkContext::parse);
                    cli::exec::run(&pool, &command, json, links.as_ref())?
                }
            };
            output.write(&command, json, &mut std::io::stdout().lock())?;
        }
        Command::ProjectArchive { action } => {
            if cli.backend != cli::BackendKind::Sql {
                return Err("project-archive requires the local SQL backend".into());
            }
            let pool = db::open(&cfg.database.path)?;
            let store = storage::AttachmentStore::from_db_path(&cfg.database.path);
            let result = match action {
                cli::ProjectArchiveAction::Export { project, out } => {
                    project_archive::export(&pool, &store, &project, &out)?
                }
                cli::ProjectArchiveAction::Import { archive, user } => {
                    project_archive::import(&pool, &store, &archive, &user)?
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        Command::Init {
            no_service,
            here,
            name,
            auth_mode,
            password,
        } => {
            // LIF-292: init/service must honor --config; they take the raw
            // flag (not the pre-loaded cfg) because init may need to CREATE
            // the file at that path and then reload anchored to it.
            return setup::cmd_init(
                cli.config.as_deref(),
                cli.db.as_deref(),
                cli.json,
                no_service,
                here,
                name,
                auth_mode,
                password,
            )
            .await;
        }

        Command::Service { action } => {
            return setup::cmd_service(&cfg, resolved_config_path.as_deref(), cli.json, &action);
        }

        Command::Dump { out } => {
            let json = cli::term::wants_json(cli.json);
            let result = dump::run_dump(&cfg.database.path, out.as_deref())
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            let m = &result.manifest;
            if json {
                let out_json = serde_json::json!({
                    "archive": result.archive_path.display().to_string(),
                    "lific_version": m.lific_version,
                    "schema_version": m.schema_version,
                    "created_at": m.created_at,
                    "db_size_bytes": m.db_size_bytes,
                    "attachment_count": m.attachment_count,
                    "attachment_bytes": m.attachment_bytes,
                });
                println!("{}", serde_json::to_string_pretty(&out_json)?);
            } else {
                use cli::ui;
                ui::step(format!(
                    "Wrote backup archive {}",
                    ui::command(result.archive_path.display())
                ));
                ui::info(ui::dim(format!(
                    "lific {} · schema v{} · db {} bytes · {} attachments ({} bytes)",
                    m.lific_version,
                    m.schema_version,
                    m.db_size_bytes,
                    m.attachment_count,
                    m.attachment_bytes
                )));
            }
            return Ok(());
        }

        Command::Restore {
            archive,
            force,
            allow_large,
        } => {
            let json = cli::term::wants_json(cli.json);
            // Best-effort warning: a hot WAL suggests the server is still up.
            if dump::server_maybe_running(&cfg.database.path) {
                eprintln!(
                    "warning: a hot -wal file is present next to {} — is the server still \
                     running? Stop it before restoring.",
                    cfg.database.path.display()
                );
            }
            let options = dump::RestoreOptions::new(force, allow_large);
            let result = dump::run_restore_with(&archive, &cfg.database.path, &options)
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            let m = &result.manifest;
            if json {
                let out_json = serde_json::json!({
                    "restored_to": result.db_path.display().to_string(),
                    "lific_version": m.lific_version,
                    "schema_version": m.schema_version,
                    "created_at": m.created_at,
                    "attachment_count": result.attachment_count,
                    "moved_existing_to": result
                        .moved_existing_to
                        .as_ref()
                        .map(|p| p.display().to_string()),
                });
                println!("{}", serde_json::to_string_pretty(&out_json)?);
            } else {
                use cli::ui;
                ui::intro("lific restore");
                ui::step(format!("Restored from {}", ui::command(archive.display())));
                ui::info(ui::dim(format!(
                    "database {} · from lific {} · schema v{} · {} attachments",
                    result.db_path.display(),
                    m.lific_version,
                    m.schema_version,
                    result.attachment_count
                )));
                if let Some(moved) = &result.moved_existing_to {
                    ui::warn(format!(
                        "previous database moved aside to {}",
                        moved.display()
                    ));
                }
                ui::outro("Start the server; any pending migrations will apply on startup.");
            }
            return Ok(());
        }

        Command::Instance { action } => {
            return cli::instance::run(&cfg, action, cli.json);
        }

        Command::Key { action } => {
            return cli::key::run(&cfg, action, cli.json);
        }

        Command::User { action } => {
            return cli::user::run(&cfg, action, cli.json);
        }

        Command::Member { action } => {
            return cli::member::run(&cfg, action, cli.json);
        }

        Command::Start {
            port,
            host,
            init_if_missing,
        } => {
            if let Some(p) = port {
                cfg.server.port = p;
            }
            if let Some(h) = host {
                cfg.server.host = h;
            }

            // LIF-468: container first boot. Guarded inside, and a no-op when
            // the database is already there. Every other startup check,
            // including the login-free/reachability refusals, still runs in
            // `server::run` exactly as before.
            if init_if_missing {
                first_boot::run(&cfg, resolved_config_source, cli.db.is_some())?;
            }

            server::run(&cfg).await?;
        }

        Command::Login {
            url,
            non_interactive,
            complete,
            label,
            no_store,
        } => {
            let json = cli::term::wants_json(cli.json);
            let args = cli::login::LoginArgs {
                url,
                non_interactive,
                complete,
                label,
                no_store,
            };
            // The login flow uses a blocking reqwest client and a polling loop
            // with sleeps; run it off the async runtime so `reqwest::blocking`
            // doesn't panic (dropping its runtime inside an async context) and
            // the sleeps don't stall the reactor.
            let cfg_clone = cfg.clone();
            tokio::task::spawn_blocking(move || cli::login::run_login(&args, &cfg_clone, json))
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            return Ok(());
        }

        Command::Logout { url } => {
            let json = cli::term::wants_json(cli.json);
            let cfg_clone = cfg.clone();
            tokio::task::spawn_blocking(move || {
                cli::login::run_logout(url.as_deref(), &cfg_clone, json)
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?
            .map_err(|e| -> Box<dyn std::error::Error> { std::io::Error::other(e).into() })?;
            return Ok(());
        }

        Command::Connect {
            clients,
            scope,
            stdio,
            oauth,
            url,
            key,
            user,
            yes,
            dry_run,
            skip_agents,
        } => {
            let json = cli::term::wants_json(cli.json);
            let scope = match scope.as_str() {
                "global" => cli::connect::clients::Scope::Global,
                "project" => cli::connect::clients::Scope::Project,
                other => {
                    return Err(format!(
                        "invalid --scope '{other}' (expected 'global' or 'project')"
                    )
                    .into());
                }
            };

            let base = cli::connect::production_base()?;
            // Refuse to conjure a fresh database in whatever directory this
            // happens to run from — connect targets an EXISTING instance.
            cli::connect::ensure_instance_exists(&cfg)?;
            let pool = db::open(&cfg.database.path)?;
            actor::set_default_transport(actor::Transport::Cli);

            let args = cli::connect::ConnectArgs {
                clients,
                scope,
                stdio,
                oauth,
                url,
                key,
                user,
                yes,
                dry_run,
                skip_agents,
            };
            if !json {
                cli::ui::intro("lific connect");
                // Say WHICH instance up front: the url clients will dial and
                // the database keys are minted in. Running from the wrong
                // directory must be obvious here, not after the writes.
                cli::ui::info(format!(
                    "Instance: {} {}",
                    cli::ui::command(cli::connect::target_url(&args, &cfg)),
                    cli::ui::dim(format!(
                        "(keys minted in {})",
                        cli::connect::absolute_db_path(&cfg)
                    ))
                ));
            }
            let result = match cli::connect::run(&args, &cfg, &pool, &base) {
                Ok(r) => r,
                Err(e) => {
                    // Close the clack session cleanly instead of leaving a
                    // dangling gutter, then surface the error normally.
                    if !json {
                        cli::ui::outro_cancel(&e);
                        std::process::exit(1);
                    }
                    return Err(e.into());
                }
            };
            cli::connect::print_result(&result, json);
            return Ok(());
        }

        Command::AgentsMd { path, project } => {
            let json = cli::term::wants_json(cli.json);
            let target = path.unwrap_or_else(|| std::path::PathBuf::from("AGENTS.md"));
            let action = cli::agents_md::write(&target, project.as_deref())?;
            if json {
                let out = serde_json::json!({
                    "path": target.display().to_string(),
                    "action": action.as_str(),
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("AGENTS.md {}: {}", action.as_str(), target.display());
            }
            return Ok(());
        }

        Command::Import { action } => {
            let json = cli::term::wants_json(cli.json);
            // The importers use blocking reqwest + polling loops; run them off
            // the async runtime so `reqwest::blocking` doesn't panic (same
            // pattern as `login`).
            let cfg_clone = cfg.clone();
            tokio::task::spawn_blocking(move || {
                cli::import::run(&cfg_clone, &action, json).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            return Ok(());
        }

        // One stdio connection, several separately authenticated instances.
        // Matched before the single-instance arms.
        Command::Mcp {
            instances: Some(instances),
            remote: _,
            url: _,
        } => {
            // clap refuses `--remote`. `--url` is refused here instead,
            // because the global `--url` also answers to LIFIC_URL and an
            // exported value must not fail a launch that never reads it.
            if cli::mcp_url_was_explicit(&matches) {
                return Err(
                    "lific mcp --instances cannot be combined with --url: every instance \
                            carries its own url in the config file"
                        .into(),
                );
            }

            // Logs on stderr only: a stray stdout line corrupts the session.
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| format!("lific={}", cfg.log.level).into()),
                )
                .with_writer(std::io::stderr)
                .init();

            return cli::mcp_instances::run(&instances).await;
        }

        Command::Mcp {
            remote: true,
            url: mcp_url,
            instances: None,
        } => {
            // LIF-453: the stdio proxy. No database, no local MCP server —
            // just JSON-RPC forwarded to a remote instance's /mcp endpoint,
            // so a remote deployment gets a local presence in an AI client.
            // Logs must stay on stderr: a stray stdout line corrupts the
            // stdio session.
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| format!("lific={}", cfg.log.level).into()),
                )
                .with_writer(std::io::stderr)
                .init();

            let url = mcp_url
                .or(cli.url)
                .map(|url| url.trim().to_owned())
                .filter(|url| !url.is_empty())
                .ok_or_else(|| -> Box<dyn std::error::Error> {
                    "lific mcp --remote needs the instance to proxy to: pass --url <URL> or set \
                     LIFIC_URL"
                        .into()
                })?;
            let credential = cli::resolve_http_credential(cli.api_key.as_deref(), || {
                cli::credentials::load(&url)
            })?;
            return cli::mcp_proxy::run(url, credential).await;
        }

        Command::Mcp {
            remote: false,
            url: _,
            instances: None,
        } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| format!("lific={}", cfg.log.level).into()),
                )
                .with_writer(std::io::stderr)
                .init();

            let pool = db::open(&cfg.database.path)?;
            info!(path = %cfg.database.path.display(), "database ready");

            // LIFIC-18: a stdio agent carries its identity in LIFIC_TOKEN. Read
            // it at startup and validate it there so a broken credential fails
            // the launch loudly instead of half-working. A missing/unbound
            // token runs as the operator with a stderr warning (MCP stdio has
            // no transport auth; the launch boundary is the trust). A
            // PRESENT-but-invalid token is a hard error: a revoked or mistyped
            // agent credential must not silently fall back to higher-privilege
            // operator access (PR #23 review).
            let manager = auth::create_key_manager()?;
            let token_user = match auth::resolve_stdio_token(&pool, &manager) {
                Ok(Some(user)) => Some(user),
                Ok(None) => {
                    // Absent or valid-but-unbound (e.g. a fresh-install
                    // unassigned key): run as the operator, with a warning.
                    eprintln!(
                        "LIFIC_TOKEN not set or unbound — this session runs as the operator, \
                         not a connected agent.\n\
                         Run `lific connect` to bind this session to an agent identity."
                    );
                    None
                }
                Err(e) => {
                    return Err(format!(
                        "LIFIC_TOKEN is set but invalid ({e}); refusing to start. A revoked \
                         or mistyped agent credential must not fall back to operator access. \
                         Re-run `lific connect` to mint a fresh token, or unset LIFIC_TOKEN \
                         to run as the operator."
                    )
                    .into());
                }
            };

            // The startup check above is a fail-fast, not the enforcement
            // point. A stdio session can run for days, so the raw token goes
            // onto the server and is re-resolved before every tool call: revoke
            // the key, change the owner's password, deactivate the account, and
            // the very next tool call fails instead of the next restart.
            let stdio_auth = std::env::var("LIFIC_TOKEN")
                .ok()
                .map(|raw| raw.trim().to_string())
                .filter(|token| !token.is_empty())
                .map(|token| mcp::StdioAuth::new(token, manager));

            // LIF-451: a stdio session launched inside a bound repository
            // defaults project-scoped tools to that project. Resolved once,
            // here, because the working directory cannot change mid-session.
            let bound_project = stdio_bound_project(&pool, token_user.as_ref());
            if let Some(ref identifier) = bound_project {
                info!(project = %identifier, "stdio session bound to project");
            }

            let server =
                mcp::LificMcp::for_stdio(pool, stdio_auth).with_bound_project(bound_project);
            let transport = rmcp::transport::io::stdio();

            info!("lific MCP server started (stdio)");
            let handle = server.serve(transport).await?;
            if let Some(u) = &token_user {
                info!(user = %u.username, "stdio session bound to agent");
            }
            handle.waiting().await?;
        }

        // These commands return before normal config/database resolution.
        Command::Completion { .. } | Command::Doctor { .. } => unreachable!(),
    }

    Ok(())
}

/// LIF-451: the project this stdio session's working directory is bound to.
///
/// Every failure is an unbound session, not an error: `lific mcp` is routinely
/// launched from a directory that is not a git repository at all, and a
/// session that simply requires an explicit `project` is a working session.
/// The one case worth a word on stderr is an ambiguous checkout, because only
/// a human can resolve it.
fn stdio_bound_project(
    pool: &db::DbPool,
    token_user: Option<&db::models::AuthUser>,
) -> Option<String> {
    use db::queries::repo_bindings::{Resolution, resolve};
    use repo_identity::AliasKind;

    let dir = std::env::current_dir().ok()?;
    let aliases = match repo_identity::compute(&dir) {
        Ok(aliases) => aliases,
        Err(e) => {
            info!(error = %e, "no repository identity here; session is unbound");
            return None;
        }
    };
    let aliases: Vec<(&str, &str)> = aliases
        .iter()
        .map(|alias| {
            let kind = match alias.kind {
                AliasKind::Remote => "remote",
                AliasKind::Root => "root",
            };
            (kind, alias.value.as_str())
        })
        .collect();

    let conn = pool.read().ok()?;
    match resolve(&conn, &aliases) {
        Ok(Resolution::One(binding)) => {
            // A LIFIC_TOKEN-scoped agent session must not have a project it
            // cannot see named in its instructions: an invisible binding is
            // treated as no binding, matching the resolve endpoint's rule.
            if let Some(user) = token_user {
                let identity = resolve_caller::ResolvedIdentity {
                    user: user.clone(),
                    transport: actor::Transport::Mcp,
                };
                match authz::can_view_project(pool, &identity, binding.project_id) {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(e) => {
                        info!(error = %e, "binding visibility check failed; session is unbound");
                        return None;
                    }
                }
            }
            db::queries::get_project(&conn, binding.project_id)
                .map(|project| project.identifier)
                .ok()
        }
        Ok(Resolution::Conflict(_)) => {
            eprintln!(
                "This repository resolves to more than one binding, so no project was assumed. \
                 Run `lific bind` in the repo to settle it."
            );
            None
        }
        Ok(Resolution::None) => None,
        Err(e) => {
            info!(error = %e, "could not resolve a repo binding; session is unbound");
            None
        }
    }
}

fn http_backend_url(cli_url: Option<&str>, public_url: Option<&str>, cfg: &Config) -> String {
    cli_url
        .or(public_url)
        .map_or_else(|| local_url(cfg), str::to_owned)
}
#[cfg(test)]
mod http_backend_url_tests {
    use super::{Config, http_backend_url};

    #[test]
    fn maps_bind_any_hosts_to_loopback() {
        let mut cfg = Config::default();
        cfg.server.host = "0.0.0.0".into();
        cfg.server.port = 4567;

        assert_eq!(http_backend_url(None, None, &cfg), "http://127.0.0.1:4567");
    }

    #[test]
    fn preserves_explicit_cli_and_public_urls() {
        let mut cfg = Config::default();
        cfg.server.public_url = Some("https://public.example.test".into());

        assert_eq!(
            http_backend_url(None, cfg.server.public_url.as_deref(), &cfg),
            "https://public.example.test"
        );
        assert_eq!(
            http_backend_url(
                Some("https://cli.example.test"),
                cfg.server.public_url.as_deref(),
                &cfg,
            ),
            "https://cli.example.test"
        );
    }
}
