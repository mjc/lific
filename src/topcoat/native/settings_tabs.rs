//! Shared navigation for account and instance settings.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) enum Tab {
    Account,
    Instance,
}

const SELECTED: &str = "relative -mb-px border-b-2 border-[var(--accent)] px-0.5 pb-2.5 pt-1 text-body font-medium text-[var(--text)]";
const UNSELECTED: &str = "relative -mb-px border-b-2 border-transparent px-0.5 pb-2.5 pt-1 text-body font-medium text-[var(--text-muted)]";

pub(crate) fn view(cx: &Cx, active: Tab, is_admin: bool) -> BoxView<'_> {
    let (account_class, instance_class) = match active {
        Tab::Account => (SELECTED, UNSELECTED),
        Tab::Instance => (UNSELECTED, SELECTED),
    };
    let account = super::navigation::attrs(cx, "/settings");
    let instance = super::navigation::attrs(cx, "/settings/instance");
    view! {
        cx =>
        <nav
            class="mb-8 flex items-center gap-6 border-b border-[var(--border)]"
            aria-label="Settings sections"
        >
            <a class=(account_class) (account)>"Account"</a>
            if is_admin {
                <a class=(instance_class) (instance)>"Instance"</a>
            }
        </nav>
    }
    .boxed()
}
