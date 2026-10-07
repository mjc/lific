//! Account Settings composition; each section owns its own controls and async state.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(super) fn content<'a>(
    cx: &'a Cx,
    account: i64,
    username: String,
    display_name: String,
    email: String,
    is_admin: bool,
) -> BoxView<'a> {
    let appearance = super::appearance::section(cx);
    let tools = super::tools::section(cx, account);
    let profile = super::profile::section(cx, account);
    let security = super::security::section(cx, account);
    let account_nav = super::super::navigation::attrs(cx, "/settings");
    let instance_nav = super::super::navigation::attrs(cx, "/settings/instance");
    let title = if display_name.is_empty() {
        username.clone()
    } else {
        display_name.clone()
    };
    let avatar = display_name
        .chars()
        .next()
        .unwrap_or_else(|| username.chars().next().unwrap_or('U'))
        .to_uppercase()
        .to_string();
    view! {
        cx =>
        <div class="flex-1 overflow-y-auto">
            <div
                class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12"
                data-native-settings="account"
            >
                <nav
                    class="mb-8 flex items-center gap-6 border-b border-[var(--border)]"
                    aria-label="Settings sections"
                >
                    <a
                        class="relative -mb-px border-b-2 border-[var(--accent)] px-0.5 pb-2.5 pt-1 text-body font-medium text-[var(--text)]"
                        (account_nav)
                    >
                        "Account"
                    </a>
                    if is_admin {
                        <a
                            class="relative -mb-px border-b-2 border-transparent px-0.5 pb-2.5 pt-1 text-body font-medium text-[var(--text-muted)]"
                            (instance_nav)
                        >
                            "Instance"
                        </a>
                    }
                </nav>
                <section class="mb-8 flex items-center gap-4">
                    <div
                        class="grid size-14 shrink-0 place-items-center rounded-full bg-[var(--accent)] font-display text-title tracking-tight text-[var(--accent-text)]"
                    >
                        (avatar)
                    </div>
                    <div class="min-w-0">
                        <h1
                            class="truncate font-display text-title leading-none tracking-tight text-[var(--text)]"
                        >
                            (title)
                        </h1>
                        <div
                            class="mt-1.5 flex flex-wrap items-center gap-2 text-body-sm"
                        >
                            <span class="font-mono text-[var(--text-muted)]">
                                "@"
                                (username.clone())
                            </span>
                            <span class="text-[var(--text-faint)]">"·"</span>
                            <span class="text-[var(--text-muted)]">(email)</span>
                            <span
                                class="rounded-full bg-[var(--bg-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase tracking-wide text-[var(--text-muted)]"
                            >
                                (if is_admin { "Admin" } else { "Member" })
                            </span>
                        </div>
                    </div>
                </section>

                (appearance)
                (tools)

                <section class="mt-10 border-t border-[var(--border)] pt-8">
                    <h2 class="mb-1 text-[1rem] font-semibold text-[var(--text)]">
                        "Account"
                    </h2>
                    <p class="mb-6 text-body leading-relaxed text-[var(--text-muted)]">
                        "Manage your profile, password, and sessions."
                    </p>
                    (profile)
                    (security)
                </section>
            </div>
        </div>
    }.boxed()
}
