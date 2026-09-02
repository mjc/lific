//! Background-service management: run the Lific server under the OS service
//! manager so it survives terminal close, logout, and reboot.
//!
//! `lific init` calls [`install`] automatically (the README's 60-second setup
//! must end with a server that's still alive tomorrow), and `lific service
//! <install|uninstall|status|stop|restart>` exposes the same machinery for
//! management.
//!
//! Two managers are supported, detected at runtime:
//!
//! - **systemd (user unit)** on Linux: `~/.config/systemd/user/lific.service`,
//!   enabled via `systemctl --user enable --now`. `loginctl enable-linger` is
//!   attempted (best-effort) so the unit keeps running after logout.
//! - **launchd (LaunchAgent)** on macOS: `~/Library/LaunchAgents/dev.lific.plist`,
//!   loaded via `launchctl bootstrap gui/<uid>` (falling back to the legacy
//!   `launchctl load -w` on older systems).
//!
//! Everything that *generates* file content is a pure function of a
//! [`ServicePlan`] so it can be unit-tested without touching systemctl.
//! Process-spawning lives in thin wrappers that the commands share.

use super::{ServiceAction, term, ui};
use crate::config::Config;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The service name / launchd label. One service per user by design: Lific's
/// target persona runs a single personal instance.
pub const SYSTEMD_UNIT_NAME: &str = "lific.service";
pub const LAUNCHD_LABEL: &str = "dev.lific";

/// Which service manager runs this platform's user services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    SystemdUser,
    Launchd,
}

impl Manager {
    pub fn label(&self) -> &'static str {
        match self {
            Manager::SystemdUser => "systemd (user unit)",
            Manager::Launchd => "launchd (LaunchAgent)",
        }
    }
}

/// Everything needed to render a service definition. All paths must be
/// absolute — service managers do not inherit the caller's cwd.
#[derive(Debug, Clone)]
pub struct ServicePlan {
    /// Absolute path to the lific binary to exec.
    pub exe: PathBuf,
    /// Absolute path to the lific.toml this instance runs from.
    pub config: PathBuf,
    /// Absolute working directory for the service process.
    pub workdir: PathBuf,
}

impl ServicePlan {
    /// Build a plan anchored at the config file, using the currently running
    /// binary as the exec target. Persistent application paths remain owned by
    /// lific.toml; this plan only records manager-owned paths.
    pub fn for_config_file(config: &Path) -> Result<Self, String> {
        let exe = service_executable()?;
        let config = config
            .canonicalize()
            .map_err(|e| format!("cannot resolve config path {}: {e}", config.display()))?;
        let workdir = config
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| format!("config path {} has no parent directory", config.display()))?
            .to_path_buf();
        reject_control_paths([exe.as_path(), config.as_path(), workdir.as_path()])?;
        Ok(Self {
            exe,
            config,
            workdir,
        })
    }

    fn argv(&self) -> [&OsStr; 4] {
        [
            self.exe.as_os_str(),
            OsStr::new("--config"),
            self.config.as_os_str(),
            OsStr::new("start"),
        ]
    }
}

/// Keep a stable launcher path such as `~/.nix-profile/bin/lific` in generated
/// services while checking that it resolves to this running binary. Nix
/// profiles update that symlink between generations; storing `current_exe()`
/// would pin the unit to one `/nix/store` path instead.
fn service_executable() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|e| format!("cannot resolve the lific binary path: {e}"))?;
    let cwd = std::env::current_dir()
        .map_err(|e| format!("cannot resolve the current directory: {e}"))?;
    let argv0 = std::env::args_os().next();
    let path = std::env::var_os("PATH");
    resolve_executable_path(argv0.as_deref(), &current, path.as_deref(), &cwd)
}

fn resolve_executable_path(
    argv0: Option<&OsStr>,
    current_exe: &Path,
    path: Option<&OsStr>,
    cwd: &Path,
) -> Result<PathBuf, String> {
    let candidate = argv0
        .and_then(|value| {
            let value_path = Path::new(value);
            if value_path.is_absolute() || value_path.components().count() > 1 {
                Some(if value_path.is_absolute() {
                    value_path.to_path_buf()
                } else {
                    cwd.join(value_path)
                })
            } else {
                path.and_then(|path| {
                    std::env::split_paths(path)
                        .map(|dir| {
                            if dir.as_os_str().is_empty() {
                                cwd.join(value_path)
                            } else if dir.is_absolute() {
                                dir.join(value_path)
                            } else {
                                cwd.join(dir).join(value_path)
                            }
                        })
                        .find(|candidate| candidate.is_file())
                })
            }
        })
        .unwrap_or_else(|| current_exe.to_path_buf());
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    };
    let current = if current_exe.is_absolute() {
        current_exe.to_path_buf()
    } else {
        cwd.join(current_exe)
    };
    reject_control_paths([candidate.as_path(), current.as_path()])?;

    let resolved = candidate.canonicalize().map_err(|e| {
        format!(
            "cannot resolve service executable {}: {e}",
            candidate.display()
        )
    })?;
    let current = current.canonicalize().map_err(|e| {
        format!(
            "cannot resolve running executable {}: {e}",
            current.display()
        )
    })?;
    if resolved != current {
        return Err(format!(
            "service executable {} does not resolve to the running lific binary {}",
            candidate.display(),
            current.display()
        ));
    }
    Ok(candidate)
}

fn reject_control_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<(), String> {
    if paths.into_iter().any(path_contains_control) {
        return Err("service paths must not contain control characters".into());
    }
    Ok(())
}

fn path_contains_control(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.to_str().is_none() || path.as_os_str().as_bytes().iter().any(u8::is_ascii_control)
    }
    #[cfg(not(unix))]
    {
        path.to_str()
            .is_none_or(|value| value.chars().any(char::is_control))
    }
}

/// Detect the available service manager. Returns None in environments without
/// one (containers, WSL without systemd, unusual inits) — callers fall back to
/// foreground `lific start`.
pub fn detect() -> Option<Manager> {
    if cfg!(target_os = "macos") {
        // launchd is the init on every supported macOS.
        return Some(Manager::Launchd);
    }
    if cfg!(target_os = "linux") {
        // A user manager needs a session bus; `systemctl --user` failing (or
        // systemctl missing) means we can't manage user units here.
        let ok = Command::new("systemctl")
            .args(["--user", "show", "--property=Version"])
            .output()
            .is_ok_and(|o| o.status.success());
        if ok {
            return Some(Manager::SystemdUser);
        }
    }
    None
}

/// Where the service definition file lives for this user.
pub fn definition_path(manager: Manager) -> Result<PathBuf, String> {
    // `dirs::home_dir()` rather than a raw `HOME` read: it is what the rest of
    // the crate uses (config.rs, credentials.rs, connect), it falls back to the
    // passwd entry when `HOME` is unset (which happens in some service and cron
    // contexts, where "HOME is not set" was a confusing way to fail), and it
    // resolves on Windows too, where a raw `HOME` read always came up empty.
    let home =
        dirs::home_dir().ok_or_else(|| "cannot determine your home directory".to_string())?;
    Ok(match manager {
        Manager::SystemdUser => home
            .join(".config")
            .join("systemd")
            .join("user")
            .join(SYSTEMD_UNIT_NAME),
        Manager::Launchd => home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist")),
    })
}

/// Render the systemd user unit. Every argument is quoted so systemd treats
/// Unix path punctuation, including apostrophes, as literal data.
pub fn systemd_unit(plan: &ServicePlan) -> String {
    let command = plan.argv().map(|arg| systemd_quote(arg, true)).join(" ");
    format!(
        r#"# Managed by `lific init` / `lific service install`.
[Unit]
Description=Lific issue tracker
After=network.target

[Service]
ExecStart={command}
WorkingDirectory={workdir}
UMask=0077
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
"#,
        workdir = systemd_quote(plan.workdir.as_os_str(), false),
    )
}

/// Quote a value for a systemd line (double quotes, escape embedded
/// quotes/backslashes). systemd's quoting rules accept this for paths.
fn systemd_quote(value: &OsStr, escape_dollar: bool) -> String {
    let s = value.to_string_lossy();
    let mut escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    if escape_dollar {
        escaped = escaped.replace('$', "$$");
    }
    format!("\"{escaped}\"")
}

/// Render the launchd LaunchAgent plist.
pub fn launchd_plist(plan: &ServicePlan, log: &Path) -> String {
    let arguments = plan
        .argv()
        .map(|arg| {
            format!(
                "        <string>{}</string>",
                xml_escape(&arg.to_string_lossy())
            )
        })
        .join("\n");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{arguments}
    </array>
    <key>WorkingDirectory</key>
    <string>{workdir}</string>
    <key>Umask</key>
    <integer>63</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        label = LAUNCHD_LABEL,
        workdir = xml_escape(&plan.workdir.display().to_string()),
        log = xml_escape(&log.display().to_string()),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn launchd_log_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "cannot determine your home directory for launchd logs".to_string())?;
    Ok(home
        .join("Library")
        .join("Logs")
        .join("Lific")
        .join("lific.log"))
}

/// The launchd domain every `launchctl` invocation below is addressed to:
/// `gui/<uid>`, the current user's GUI session.
///
/// This is the crate's only use of `libc` outside the `#[cfg(unix)]` SIGPIPE
/// setup in `main.rs`, and `libc` is declared as a unix-only dependency. The
/// non-unix arm exists so the crate still *typechecks* on Windows, where
/// `detect()` never yields `Manager::Launchd` and none of this can run
/// (reported as GitHub #20: v2.4.0 failed to compile on Windows because five
/// `libc::getuid()` call sites were spread through this module unguarded).
///
/// It returns an error rather than `unreachable!()` because `Manager` is a
/// public enum: a caller can construct `Launchd` on any platform, and a
/// wrong-platform request deserves a message, not a panic.
#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the non-unix implementation can fail and callers are platform-neutral"
)]
fn launchd_domain() -> Result<String, String> {
    // POSIX guarantees getuid() always succeeds and sets no errno.
    Ok(format!("gui/{}", unsafe { libc::getuid() }))
}

#[cfg(not(unix))]
fn launchd_domain() -> Result<String, String> {
    Err("launchd services are only available on macOS".to_string())
}

/// Load (or reload) the LaunchAgent at `path` into the user's GUI domain.
///
/// Shared by [`install`] and [`restart`], which perform an identical dance:
/// boot out any already-loaded copy, bootstrap the new one, and fall back to
/// the legacy `launchctl load -w` on macOS versions without `bootstrap`.
fn launchd_bootstrap(path: &Path) -> Result<(), String> {
    let domain = launchd_domain()?;
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{LAUNCHD_LABEL}")])
        .output();
    let boot = Command::new("launchctl")
        .args(["bootstrap", &domain, &path.display().to_string()])
        .output()
        .map_err(|e| format!("launchctl failed to run: {e}"))?;
    if !boot.status.success() {
        run_ok("launchctl", &["load", "-w", &path.display().to_string()])?;
    }
    Ok(())
}

/// Outcome of an install, for both human and JSON output.
#[derive(Debug, serde::Serialize)]
pub struct InstallReport {
    pub manager: String,
    pub definition: String,
    /// Linux only: whether lingering was enabled so the service survives
    /// logout. None when not applicable/attempted.
    pub linger: Option<bool>,
}

/// Write the service definition and start it now + on boot.
pub fn install(manager: Manager, plan: &ServicePlan) -> Result<InstallReport, String> {
    let path = definition_path(manager)?;
    crate::secure_fs::ensure_private_parent(&path)
        .map_err(|e| format!("cannot create service directory: {e}"))?;
    match manager {
        Manager::SystemdUser => {
            crate::secure_fs::atomic_replace_private(&path, systemd_unit(plan).as_bytes())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            run_ok("systemctl", &["--user", "daemon-reload"])?;
            run_ok(
                "systemctl",
                &["--user", "enable", "--now", SYSTEMD_UNIT_NAME],
            )?;
            // Best-effort: without lingering, user units stop at logout, which
            // defeats the point on a personal box. May prompt for auth or be
            // refused; that's fine — report, don't fail.
            let linger = Command::new("loginctl")
                .arg("enable-linger")
                .status()
                .is_ok_and(|s| s.success());
            Ok(InstallReport {
                manager: manager.label().into(),
                definition: path.display().to_string(),
                linger: Some(linger),
            })
        }
        Manager::Launchd => {
            let log = launchd_log_path()?;
            reject_control_paths([log.as_path()])?;
            crate::secure_fs::ensure_private_parent(&log)
                .map_err(|e| format!("cannot create log directory: {e}"))?;
            if let Some(dir) = log.parent() {
                crate::secure_fs::set_private_dir(dir)
                    .map_err(|e| format!("cannot protect {}: {e}", dir.display()))?;
            }
            crate::secure_fs::atomic_replace_private(&path, launchd_plist(plan, &log).as_bytes())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            launchd_bootstrap(&path)?;
            Ok(InstallReport {
                manager: manager.label().into(),
                definition: path.display().to_string(),
                linger: None,
            })
        }
    }
}

/// Stop the service and remove its definition.
pub fn uninstall(manager: Manager) -> Result<String, String> {
    let path = definition_path(manager)?;
    match manager {
        Manager::SystemdUser => {
            // disable --now both stops and removes boot wiring; ignore failure
            // (unit may already be gone) but surface file-removal errors.
            let _ = Command::new("systemctl")
                .args(["--user", "disable", "--now", SYSTEMD_UNIT_NAME])
                .output();
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
            }
            let _ = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .output();
        }
        Manager::Launchd => {
            let domain = launchd_domain()?;
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{domain}/{LAUNCHD_LABEL}")])
                .output();
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
            }
        }
    }
    Ok(path.display().to_string())
}

#[derive(Debug, serde::Serialize)]
pub struct StatusReport {
    pub manager: String,
    pub state: ServiceState,
    pub definition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NotInstalled,
    InstalledStopped,
    InstalledRunning,
    OrphanedRunning,
}

/// Is the service installed and currently running?
pub fn status(manager: Manager) -> Result<StatusReport, String> {
    let path = definition_path(manager)?;
    let active = match manager {
        Manager::SystemdUser => Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", SYSTEMD_UNIT_NAME])
            .status()
            .is_ok_and(|s| s.success()),
        Manager::Launchd => Command::new("launchctl")
            .args(["print", &format!("{}/{LAUNCHD_LABEL}", launchd_domain()?)])
            .output()
            .is_ok_and(|o| o.status.success()),
    };
    let state = match (path.exists(), active) {
        (false, false) => ServiceState::NotInstalled,
        (true, false) => ServiceState::InstalledStopped,
        (true, true) => ServiceState::InstalledRunning,
        (false, true) => ServiceState::OrphanedRunning,
    };
    Ok(StatusReport {
        manager: manager.label().into(),
        state,
        definition: path.display().to_string(),
    })
}

/// Stop (without uninstalling).
pub fn stop(manager: Manager) -> Result<(), String> {
    match manager {
        Manager::SystemdUser => run_ok("systemctl", &["--user", "stop", SYSTEMD_UNIT_NAME]),
        Manager::Launchd => {
            let domain = launchd_domain()?;
            run_ok(
                "launchctl",
                &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
            )
        }
    }
}

/// Restart (or start, if stopped).
pub fn restart(manager: Manager) -> Result<(), String> {
    match manager {
        Manager::SystemdUser => run_ok("systemctl", &["--user", "restart", SYSTEMD_UNIT_NAME]),
        Manager::Launchd => launchd_bootstrap(&definition_path(manager)?),
    }
}

/// The hint we print for reading service logs on this platform.
pub fn logs_hint(manager: Manager) -> String {
    match manager {
        Manager::SystemdUser => format!("journalctl --user -u {SYSTEMD_UNIT_NAME} -f"),
        Manager::Launchd => launchd_log_path().map_or_else(
            |_| "open ~/Library/Logs/Lific/lific.log".into(),
            |path| format!("tail -f {}", path.display()),
        ),
    }
}

/// Run a `lific service` action without loading ambient application config.
/// Only installation resolves and validates the explicitly selected instance.
pub fn run(
    config_flag: Option<&Path>,
    db_flag: Option<&Path>,
    json_flag: bool,
    action: &ServiceAction,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = term::wants_json(json_flag);
    if matches!(action, ServiceAction::Install) && db_flag.is_some() {
        return Err(
            "`lific service install --db` would create a second persistent path authority; set database.path in lific.toml instead"
                .into(),
        );
    }
    let Some(manager) = detect() else {
        return Err(
            "no supported service manager found (needs a systemd user session on Linux, or launchd on macOS)"
                .into(),
        );
    };

    match action {
        ServiceAction::Install => {
            let cwd = std::env::current_dir()?;
            let config_path = config_flag.map_or_else(
                || Config::discover_path().unwrap_or_else(|| PathBuf::from("lific.toml")),
                Path::to_path_buf,
            );
            let config_path = crate::secure_fs::absolutize(&config_path, &cwd);
            if config_flag.is_none() && !config_path.exists() {
                return Err(format!(
                    "config not found at {} — run `lific init` first (or point --config at an existing lific.toml)",
                    config_path.display()
                )
                .into());
            }
            // An explicit --config path is an initialization authority: load
            // it here so a missing writable file gets a private default before
            // the service plan canonicalizes it. Ambient discovery still
            // requires an existing config and keeps `service install` from
            // silently creating an unrelated instance.
            Config::load(Some(&config_path))?;
            let plan = ServicePlan::for_config_file(&config_path)?;
            let report = install(manager, &plan)?;
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
                        "`loginctl enable-linger` didn't succeed — the service will stop when you log out. Run it manually to fix that.",
                    );
                }
                ui::outro(format!("Logs: {}", ui::command(logs_hint(manager))));
            }
        }
        ServiceAction::Uninstall => {
            let removed = uninstall(manager)?;
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
            let status = status(manager)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                match status.state {
                    ServiceState::InstalledRunning => {
                        ui::step(format!("Service is running ({})", status.manager));
                    }
                    ServiceState::InstalledStopped => ui::error(format!(
                        "Service is installed but NOT running ({}). Start it: {}",
                        status.manager,
                        ui::command("lific service restart")
                    )),
                    ServiceState::NotInstalled => ui::error(format!(
                        "Service is not installed. Install it: {}",
                        ui::command("lific service install")
                    )),
                    ServiceState::OrphanedRunning => ui::error(format!(
                        "Service is running but its definition is missing ({}). Reinstall it: {}",
                        status.manager,
                        ui::command("lific service install")
                    )),
                }
            }
            if status.state != ServiceState::InstalledRunning {
                return Err("service is not installed and running".into());
            }
        }
        ServiceAction::Stop => {
            stop(manager)?;
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
            restart(manager)?;
            if json {
                println!("{}", serde_json::json!({ "restarted": true }));
            } else {
                ui::step("Service restarted");
            }
        }
    }
    Ok(())
}

fn run_ok(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd} failed to run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{cmd} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ServicePlan {
        ServicePlan {
            exe: PathBuf::from("/home/u/.cargo/bin/lific"),
            config: PathBuf::from("/home/u/tracker/lific.toml"),
            workdir: PathBuf::from("/home/u/tracker"),
        }
    }

    fn launchd_log() -> PathBuf {
        PathBuf::from("/home/u/Library/Logs/Lific/lific.log")
    }

    #[test]
    fn service_argv_has_one_canonical_global_argument_order() {
        let args = plan().argv().map(|arg| arg.to_string_lossy().into_owned());
        assert_eq!(
            args,
            [
                "/home/u/.cargo/bin/lific",
                "--config",
                "/home/u/tracker/lific.toml",
                "start",
            ]
        );
    }

    #[test]
    fn install_rejects_database_override_before_manager_lookup() {
        let err = run(
            None,
            Some(Path::new("other.db")),
            true,
            &ServiceAction::Install,
        )
        .unwrap_err();
        assert!(err.to_string().contains("set database.path"));
    }

    #[test]
    fn systemd_unit_execs_start_with_absolute_config() {
        let unit = systemd_unit(&plan());
        assert!(
            unit.contains(
                r#"ExecStart="/home/u/.cargo/bin/lific" "--config" "/home/u/tracker/lific.toml" "start""#
            ),
            "unit was:\n{unit}"
        );
        assert!(unit.contains(r#"WorkingDirectory="/home/u/tracker""#));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("UMask=0077"));
    }

    #[test]
    fn systemd_unit_quotes_paths_with_spaces() {
        let mut p = plan();
        p.exe = PathBuf::from("/home/u/my tools/lific");
        p.config = PathBuf::from("/home/u/my tracker/lific.toml");
        let unit = systemd_unit(&p);
        assert!(
            unit.contains(
                r#"ExecStart="/home/u/my tools/lific" "--config" "/home/u/my tracker/lific.toml" "start""#
            ),
            "unit was:\n{unit}"
        );
    }

    #[test]
    fn systemd_unit_quotes_apostrophes_in_paths() {
        let mut p = plan();
        p.exe = PathBuf::from("/home/o'connor/bin/lific");
        p.config = PathBuf::from("/home/o'connor/lific.toml");
        p.workdir = PathBuf::from("/home/o'connor");
        let unit = systemd_unit(&p);
        assert!(
            unit.contains(
                r#"ExecStart="/home/o'connor/bin/lific" "--config" "/home/o'connor/lific.toml" "start""#
            ),
            "unit was:\n{unit}"
        );
        assert!(unit.contains(r#"WorkingDirectory="/home/o'connor""#));
    }

    #[test]
    fn systemd_unit_escapes_expansion_characters() {
        let mut p = plan();
        p.exe = PathBuf::from("/home/u/bin/$lific%bin");
        p.config = PathBuf::from("/home/u/config/$env%file.toml");
        p.workdir = PathBuf::from("/home/u/$data%dir");
        let unit = systemd_unit(&p);
        assert!(
            unit.contains("/home/u/bin/$$lific%%bin"),
            "unit was:\n{unit}"
        );
        assert!(
            unit.contains("/home/u/config/$$env%%file.toml"),
            "unit was:\n{unit}"
        );
        assert!(unit.contains("/home/u/$data%%dir"), "unit was:\n{unit}");
    }

    #[test]
    fn config_path_with_control_character_is_rejected() {
        // Validate the path before touching the filesystem: Windows cannot
        // create a filename containing a newline in order to reach the same
        // check through `canonicalize`.
        let config = PathBuf::from(format!("lific_svc_control_{}\n", std::process::id()));
        let error = reject_control_paths([config.as_path()]).unwrap_err();
        assert!(error.contains("control characters"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_service_path_is_rejected() {
        use std::os::unix::ffi::OsStringExt;

        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', 0xff,
        ]));
        assert!(path_contains_control(&path));
    }

    #[test]
    fn launchd_plist_contains_program_arguments_and_label() {
        let plist = launchd_plist(&plan(), &launchd_log());
        assert!(plist.contains("<string>dev.lific</string>"));
        assert!(plist.contains("<string>/home/u/.cargo/bin/lific</string>"));
        assert!(plist.contains("<string>start</string>"));
        assert!(plist.contains("<string>--config</string>"));
        assert!(plist.contains("<string>/home/u/tracker/lific.toml</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>Umask</key>\n    <integer>63</integer>"));
        assert!(plist.contains("<key>WorkingDirectory</key>"));
        assert!(plist.contains("/home/u/Library/Logs/Lific/lific.log"));
        assert!(!plist.contains("/home/u/tracker/lific.log"));
    }

    #[test]
    fn launchd_plist_escapes_xml_special_chars() {
        let mut p = plan();
        p.workdir = PathBuf::from("/home/u/a&b");
        let plist = launchd_plist(&p, &launchd_log());
        assert!(plist.contains("/home/u/a&amp;b"));
        assert!(!plist.contains("<string>/home/u/a&b</string>"));
    }

    // LIF-292: the plan must anchor to the config file itself, so `--config
    // /elsewhere/lific.toml` produces a unit that points at /elsewhere — not
    // at a lific.toml in whatever directory the command happened to run in.
    #[test]
    fn for_config_file_anchors_workdir_to_the_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = dir.join("lific.toml");
        std::fs::write(&config, "").unwrap();

        let plan = ServicePlan::for_config_file(&config).unwrap();
        let canon_dir = dir.canonicalize().unwrap();
        assert_eq!(plan.workdir, canon_dir);
        assert_eq!(plan.config, canon_dir.join("lific.toml"));
    }

    #[test]
    fn for_config_file_errors_on_missing_config() {
        let err =
            ServicePlan::for_config_file(Path::new("/tmp/nonexistent_lific_dir_12345/lific.toml"))
                .unwrap_err();
        assert!(err.contains("cannot resolve config path"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn executable_resolution_preserves_a_profile_symlink() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let profile = tmp.path().join("profile/bin");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let current = store.join("lific");
        let stable = profile.join("lific");
        std::fs::write(&current, "binary").unwrap();
        symlink(&current, &stable).unwrap();

        let resolved =
            resolve_executable_path(Some(stable.as_os_str()), &current, None, tmp.path()).unwrap();
        assert_eq!(resolved, stable);
    }

    // The five launchctl call sites used to inline `gui/{uid}` themselves; they
    // now share this one helper, so its shape is what keeps `launchctl` able to
    // address the agent. (The non-unix arm is what makes the crate compile on
    // Windows at all - GitHub #20 - and is covered by CI's windows job.)
    #[cfg(unix)]
    #[test]
    fn launchd_domain_addresses_the_current_users_gui_session() {
        let domain = launchd_domain().unwrap();
        let uid = domain
            .strip_prefix("gui/")
            .unwrap_or_else(|| panic!("expected gui/<uid>, got: {domain}"));
        assert!(
            !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit()),
            "expected gui/<uid>, got: {domain}"
        );
    }

    #[test]
    fn definition_paths_are_per_user() {
        // HOME is set in test environments; just sanity-check the shape.
        let systemd = definition_path(Manager::SystemdUser).unwrap();
        assert!(systemd.ends_with(".config/systemd/user/lific.service"));
        let launchd = definition_path(Manager::Launchd).unwrap();
        assert!(launchd.ends_with("Library/LaunchAgents/dev.lific.plist"));
    }
}
