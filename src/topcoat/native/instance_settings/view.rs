//! Native instance settings and member roster initial document.

use topcoat::{
    context::Cx,
    runtime::{record, shard},
    view::{BoxView, View, ViewExt, view},
};

use super::super::{context, session};

#[record]
#[derive(Clone)]
struct RosterEntry {
    username: String,
    display_name: String,
    initials: String,
    is_admin: bool,
    is_active: bool,
}

pub(super) fn content(cx: &Cx, account: i64, is_admin: bool) -> BoxView<'_> {
    let tabs =
        super::super::settings_tabs::view(cx, super::super::settings_tabs::Tab::Instance, is_admin);
    let back_to_account = super::super::navigation::attrs(cx, "/settings");
    if !is_admin {
        return view! {
            cx =>
            <div class="flex-1 overflow-y-auto">
                <div class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12">
                    (tabs)
                    <section
                        class="mx-auto flex max-w-[440px] flex-col items-center py-20 text-center"
                    >
                        <h1 class="text-[1rem] font-semibold text-[var(--text)]">
                            "Admins only"
                        </h1>
                        <p class="mt-1 text-body text-[var(--text-muted)]">
                            "Instance settings are visible to administrators of this instance."
                        </p>
                        <a
                            class="mt-5 rounded-md bg-[var(--btn-success)] px-3 py-1.5 text-body-sm font-medium text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]"
                            (back_to_account)
                        >
                            "Back to account"
                        </a>
                    </section>
                </div>
            </div>
        }
        .boxed();
    }
    view! { cx => native_instance_settings(account: account) }.boxed()
}

#[shard("/__native_instance_settings/page")]
async fn native_instance_settings(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    if !user.is_admin {
        return Err(crate::error::LificError::Forbidden("only an admin can do this".into()).into());
    }
    let db = context::db(cx);
    let settings = session::read(cx, crate::db::queries::settings::get(&*db.read()?))?;
    let users = session::read(
        cx,
        crate::services::project_form::list_leads(db, &caller.identity),
    )?;
    let instance_name = settings
        .instance_name
        .unwrap_or_else(|| "Use the host name".into());
    let signup_status = if settings.allow_signup {
        "Open"
    } else {
        "Closed"
    };
    let domains = if settings.signup_email_domains.is_empty() {
        "Any email domain".to_owned()
    } else {
        settings.signup_email_domains.join(", ")
    };
    let session_lifetime = format!("{} days", settings.session_lifetime_days);
    let login_message = settings.login_message.unwrap_or_else(|| "None".into());
    let single_user = if settings.web_auto_login {
        "Enabled"
    } else {
        "Sign-in required"
    };
    let authorization = if settings.authz_enforced {
        "Enforced"
    } else {
        "Legacy access"
    };
    let roster = users
        .into_iter()
        .map(|user| {
            let display_name = if user.display_name.trim().is_empty() {
                user.username.clone()
            } else {
                user.display_name.clone()
            };
            RosterEntry {
                username: user.username,
                initials: initials(&display_name),
                display_name,
                is_admin: user.is_admin,
                is_active: user.is_active,
            }
        })
        .collect::<Vec<_>>();
    let people_count = roster.len();
    let admin_count = roster
        .iter()
        .filter(|user| user.is_admin && user.is_active)
        .count();
    let singular_people = people_count == 1;
    Ok(view! {
        cx =>
        <div class="flex-1 overflow-y-auto">
            <div
                class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12"
                data-native-instance-settings=""
            >
                (super::super::settings_tabs::view(
                    cx,
                    super::super::settings_tabs::Tab::Instance,
                    true,
                ))
                <section class="mb-8">
                    <h1
                        class="font-display text-title tracking-tight text-[var(--text)] leading-none"
                    >
                        "Instance"
                    </h1>
                    <p class="mt-2 text-body text-[var(--text-muted)]">
                        "Settings for this Lific instance."
                    </p>
                </section>
                <section
                    class="rounded-xl bg-[var(--surface)] p-5 shadow-sm"
                    aria-labelledby="instance-settings-heading"
                >
                    <h2
                        id="instance-settings-heading"
                        class="mb-5 text-body-lg font-semibold text-[var(--text)]"
                    >
                        "Settings"
                    </h2>
                    <dl class="grid max-w-[640px] gap-4 sm:grid-cols-2">
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Instance name"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (instance_name)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Sign-ups"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (signup_status)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Allowed signup domains"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">(domains)</dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Session lifetime"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (session_lifetime)
                            </dd>
                        </div>
                        <div class="sm:col-span-2">
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Login message"
                            </dt>
                            <dd
                                class="mt-1 whitespace-pre-wrap text-body text-[var(--text)]"
                            >
                                (login_message)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Single-user mode"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (single_user)
                            </dd>
                        </div>
                        <div>
                            <dt
                                class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                            >
                                "Project authorization"
                            </dt>
                            <dd class="mt-1 text-body text-[var(--text)]">
                                (authorization)
                            </dd>
                        </div>
                    </dl>
                </section>
                <section class="mt-10" aria-labelledby="instance-members-heading">
                    <h2
                        id="instance-members-heading"
                        class="text-[1rem] font-semibold text-[var(--text)]"
                    >
                        "Members"
                    </h2>
                    <p class="mb-5 mt-1 text-body text-[var(--text-muted)]">
                        (people_count)
                        " "
                        if singular_people {
                            "person"
                        } else {
                            "people"
                        }
                        " on this instance · "
                        (admin_count)
                        " admin."
                    </p>
                    <div
                        class="overflow-hidden rounded-xl bg-[var(--surface)] shadow-sm"
                    >
                        for user in roster.iter() {
                            <div
                                class=(if user.is_active {
                                    "flex items-center gap-3 px-4 py-3"
                                } else {
                                    "flex items-center gap-3 px-4 py-3 opacity-60"
                                })
                            >
                                <div
                                    class="grid size-8 shrink-0 place-items-center rounded-full bg-[var(--accent)] text-micro font-semibold text-[var(--accent-text)]"
                                >
                                    (user.initials.clone())
                                </div>
                                <div class="min-w-0 flex-1">
                                    <div
                                        class="truncate leading-tight text-body text-[var(--text)]"
                                    >
                                        (user.display_name.clone())
                                    </div>
                                    <div
                                        class="mt-0.5 truncate font-mono text-caption text-[var(--text-faint)]"
                                    >
                                        "@"
                                        (user.username.clone())
                                    </div>
                                </div>
                                if !user.is_active {
                                    <span
                                        class="shrink-0 rounded-full bg-[var(--warn-bg)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--warn-text)]"
                                    >
                                        "Deactivated"
                                    </span>
                                }
                                <span
                                    class=(if user.is_admin {
                                        "shrink-0 rounded-full bg-[var(--accent-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--accent)]"
                                    } else {
                                        "shrink-0 rounded-full bg-[var(--bg-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase text-[var(--text-muted)]"
                                    })
                                >
                                    {
                                        if user.is_admin {
                                            "Admin"
                                        } else {
                                            "Member"
                                        }
                                    }
                                </span>
                            </div>
                        }
                    </div>
                    <p
                        class="mt-2 max-w-[60ch] text-caption leading-relaxed text-[var(--text-faint)]"
                    >
                        "Deactivating an account ends its sessions and revokes its API keys and tokens. Nothing it wrote is removed. The last admin who can still sign in cannot be demoted or deactivated."
                    </p>
                </section>
            </div>
        </div>
    })
}

fn initials(name: &str) -> String {
    name.split([' ', '_', '-'])
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}
