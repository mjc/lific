//! Account Settings composition; each section owns its own controls and async state.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(super) fn content<'a>(
    cx: &'a Cx,
    profile_handles: super::super::account_profile::Handles,
    is_admin: bool,
) -> BoxView<'a> {
    let account = profile_handles.0;
    let appearance = super::appearance::section(cx);
    let tools = super::tools::section(cx, account);
    let profile = super::profile::section(cx, profile_handles.clone());
    let security = super::security::section(cx, account);
    let account_nav = super::super::navigation::attrs(cx, "/settings");
    let instance_nav = super::super::navigation::attrs(cx, "/settings/instance");
    let account_header = super::super::account_profile::header(cx, profile_handles);
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
                (account_header)

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
