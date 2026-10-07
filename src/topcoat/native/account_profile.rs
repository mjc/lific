//! Account display state belongs to the workspace, independently of page drafts.

use super::{context, home_shell, session};
use crate::db::models::User;
use topcoat::{
    context::Cx,
    runtime::{Signal, record, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

#[record]
#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub is_admin: bool,
}

impl From<User> for Profile {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            email: user.email,
            is_admin: user.is_admin,
        }
    }
}

pub(crate) type Handles = (i64, Signal<Profile>, Signal<usize>);

pub(crate) fn load(cx: &Cx, account: i64) -> topcoat::Result<Handles> {
    let profile = session::read(
        cx,
        crate::db::queries::users::get_user_by_id(&*context::db(cx).read()?, account),
    )?;
    let owner = cx.keyed((account, "account-profile"));
    Ok((
        account,
        signal(&owner, || Profile::from(profile)),
        signal(&owner, || 0usize),
    ))
}

fn authorize(cx: &Cx, account: i64) -> topcoat::Result<()> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    Ok(())
}

pub(crate) fn link(cx: &Cx, handles: Handles, mobile: bool) -> BoxView<'_> {
    let (account, profile, _) = handles;
    view! {
        cx =>
        native_account_identity(
            account: account,
            profile: $(profile.get()),
            mobile: mobile
        )
    }
    .boxed()
}

#[shard("/__native_account/identity")]
async fn native_account_identity(
    cx: &Cx,
    account: i64,
    profile: Profile,
    mobile: bool,
) -> topcoat::Result<impl View> {
    authorize(cx, account)?;
    let name =
        super::avatar::display_name(Some(&profile.display_name), Some(&profile.username), "")
            .to_owned();
    let initials = home_shell::display_initials(&profile.display_name, &profile.username);
    Ok(home_shell::account_link(cx, name, initials, mobile))
}

pub(crate) fn header(cx: &Cx, handles: Handles) -> BoxView<'_> {
    let (account, profile, _) = handles;
    view! { cx => native_account_header(account: account, profile: $(profile.get())) }.boxed()
}

#[shard("/__native_account/header")]
async fn native_account_header(
    cx: &Cx,
    account: i64,
    profile: Profile,
) -> topcoat::Result<impl View> {
    authorize(cx, account)?;
    let title =
        super::avatar::display_name(Some(&profile.display_name), Some(&profile.username), "")
            .to_owned();
    let avatar = title
        .chars()
        .next()
        .unwrap_or('U')
        .to_uppercase()
        .to_string();
    Ok(view! {
        cx =>
        <section class="mb-8 flex items-center gap-4" data-native-account-header="">
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
                <div class="mt-1.5 flex flex-wrap items-center gap-2 text-body-sm">
                    <span class="font-mono text-[var(--text-muted)]">
                        "@"
                        (profile.username)
                    </span>
                    <span class="text-[var(--text-faint)]">"·"</span>
                    <span class="text-[var(--text-muted)]">(profile.email)</span>
                    <span
                        class="rounded-full bg-[var(--bg-subtle)] px-1.5 py-0.5 text-micro font-semibold uppercase tracking-wide text-[var(--text-muted)]"
                    >
                        (if profile.is_admin { "Admin" } else { "Member" })
                    </span>
                </div>
            </div>
        </section>
    }.boxed())
}
