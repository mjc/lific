//! Account Settings composition; each section owns its own controls and async state.

use topcoat::{
    context::Cx,
    runtime::signal,
    view::{BoxView, ViewExt, view},
};

pub(super) fn content<'a>(
    cx: &'a Cx,
    profile_handles: super::super::account_profile::Handles,
    is_admin: bool,
) -> BoxView<'a> {
    let account = profile_handles.0;
    let tools_revision = signal(&cx.keyed((account, "tools-revision")), || 0_usize);
    let appearance = super::appearance::section(cx);
    let tools = super::tools::section(cx, account, tools_revision.clone());
    let profile = super::profile::section(cx, profile_handles.clone());
    let security = super::security::section(cx, account, tools_revision);
    let settings_tabs =
        super::super::settings_tabs::view(cx, super::super::settings_tabs::Tab::Account, is_admin);
    let account_header = super::super::account_profile::header(cx, profile_handles);
    view! {
        cx =>
        <div class="flex-1 overflow-y-auto">
            <div
                class="mx-auto w-full max-w-[1000px] px-6 py-10 md:py-12"
                data-native-settings="account"
            >
                (settings_tabs)
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
    }
    .boxed()
}
