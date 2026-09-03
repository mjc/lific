//! `lific instance <action>`: read and edit instance-wide settings.
//!
//! The settings row in the database is authoritative; TOML only seeds it on
//! first touch (LIF-210).

use crate::cli::{InstanceAction, term, ui};
use crate::config::Config;
use crate::db;

pub fn run(
    cfg: &Config,
    action: InstanceAction,
    json_flag: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = term::wants_json(json_flag);
    let pool = db::open(&cfg.database.path)?;
    // Seed the settings row from TOML on first touch, then operate on
    // the DB store (authoritative).
    {
        let conn = pool.write()?;
        db::queries::settings::ensure(&conn, cfg.auth.allow_signup)?;
    }

    match action {
        InstanceAction::Set {
            name,
            signups,
            signup_domains,
            session_days,
            login_message,
            auto_login,
            authz_enforced,
        } => {
            let patch = db::queries::settings::InstanceSettingsPatch {
                allow_signup: signups,
                instance_name: name,
                signup_email_domains: signup_domains.map(|csv| {
                    csv.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                }),
                session_lifetime_days: session_days,
                login_message,
                web_auto_login: auto_login,
                authz_enforced,
            };
            let conn = pool.write()?;
            db::queries::settings::update(&conn, patch)?;
            drop(conn);
            if !json {
                println!("Updated instance settings.");
            }
            // Fall through to print current state below.
        }
        InstanceAction::Info => {}
    }

    let settings = {
        let conn = pool.read()?;
        db::queries::settings::get(&conn)?
    };
    let (total, admins) = {
        let conn = pool.read()?;
        let users = db::queries::users::list_users(&conn)?;
        let humans: Vec<_> = users.iter().filter(|u| !u.is_bot).collect();
        let admins = humans.iter().filter(|u| u.is_admin).count();
        (humans.len(), admins)
    };
    let version = env!("CARGO_PKG_VERSION");
    let domains = if settings.signup_email_domains.is_empty() {
        "(any)".to_string()
    } else {
        settings.signup_email_domains.join(", ")
    };

    if json {
        let out = serde_json::json!({
            "version": version,
            "database": cfg.database.path.display().to_string(),
            "host": cfg.server.host,
            "port": cfg.server.port,
            "public_url": cfg.server.public_url,
            "name": settings.instance_name,
            "allow_signup": settings.allow_signup,
            "signup_email_domains": settings.signup_email_domains,
            "session_lifetime_days": settings.session_lifetime_days,
            "login_message": settings.login_message,
            "users": { "total": total, "admins": admins },
        });
        println!("{}", crate::cli::term::json_string(&out)?);
    } else {
        ui::line("Instance");
        ui::line(format!(
            "  name:          {}",
            settings.instance_name.as_deref().unwrap_or("(unnamed)")
        ));
        ui::line(format!("  version:       {version}"));
        ui::line(format!("  database:      {}", cfg.database.path.display()));
        ui::line(format!(
            "  bind:          {}:{}",
            cfg.server.host, cfg.server.port
        ));
        ui::line(format!(
            "  public url:    {}",
            cfg.server.public_url.as_deref().unwrap_or("(not set)")
        ));
        ui::line(format!(
            "  signups:       {}",
            if settings.allow_signup {
                "open"
            } else {
                "closed"
            }
        ));
        ui::line(format!("  signup domains:{domains}"));
        ui::line(format!(
            "  session days:  {}",
            settings.session_lifetime_days
        ));
        ui::line(format!(
            "  login message: {}",
            settings.login_message.as_deref().unwrap_or("(none)")
        ));
        ui::line(format!("  users:         {total} ({admins} admin)"));
    }
    Ok(())
}
