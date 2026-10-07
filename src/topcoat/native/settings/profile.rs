use super::super::{context, session};
use super::actions::save_profile;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section<'a>(cx: &'a Cx, account: i64) -> BoxView<'a> {
    view! { cx => native_settings_profile(account: account) }.boxed()
}

#[shard("/__native_settings/profile")]
async fn native_settings_profile(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let _caller = session::read(cx, super::actions::same_account(cx, account))?;
    let profile = session::read(
        cx,
        crate::db::queries::users::get_user_by_id(&*context::db(cx).read()?, account),
    )?;
    Ok(render_section(
        cx,
        account,
        profile.username,
        profile.display_name,
        profile.email,
    ))
}

fn save_attrs(
    cx: &Cx,
    account: i64,
    name: Signal<String>,
    email: Signal<String>,
    saving: Signal<bool>,
    saved: Signal<bool>,
    error: Signal<String>,
) -> Attributes {
    let failed_saving = saving.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !saving.get() {
            saving.set(true);
            saved.set(false);
            error.set("".to_owned());
            let requested_name = name.get();
            let requested_email = email.get();
            let _failed = || {
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                failed_saving.set(false);
                failed_error.set("Couldn't save your profile. Try again.".to_owned());
            };
            let _run = async || {
                let result = save_profile(account, requested_name, requested_email).await;
                if raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    return;
                }
                if result.0 {
                    saved.set(true);
                } else {
                    error.set(result.1);
                }
                saving.set(false);
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn render_section<'a>(
    cx: &'a Cx,
    account: i64,
    username: String,
    display_name: String,
    email: String,
) -> BoxView<'a> {
    let state_cx = cx.keyed((account, "profile"));
    let profile_name = signal(&state_cx, || display_name.clone());
    let profile_email = signal(&state_cx, || email.clone());
    let saving = signal(&state_cx, || false);
    let saved = signal(&state_cx, || false);
    let error = signal(&state_cx, String::new);
    let save = save_attrs(
        cx,
        account,
        profile_name.clone(),
        profile_email.clone(),
        saving.clone(),
        saved.clone(),
        error.clone(),
    );
    view! {
        cx =>
        <section>
            <div class="flex max-w-[480px] flex-col gap-3.5">
                <label class="block">
                    <span
                        class="mb-1.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                    >
                        "Display name"
                    </span>
                    <input
                        class=(super::INPUT)
                        :value=$(profile_name.get())
                        @input=$(|event: Event| profile_name.set(
                                event.target.value.to_owned(),
                            ))
                    />
                </label>
                <label class="block">
                    <span
                        class="mb-1.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                    >
                        "Email"
                    </span>
                    <input
                        type="email"
                        class=(super::INPUT)
                        :value=$(profile_email.get())
                        @input=$(|event: Event| profile_email.set(
                                event.target.value.to_owned(),
                            ))
                    />
                </label>
                <div>
                    <span
                        class="mb-1.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                    >
                        "Username"
                    </span>
                    <p class="font-mono text-body text-[var(--text-muted)]">
                        "@"
                        (username)
                    </p>
                </div>
            </div>
            <p class="mt-2.5 text-caption text-[var(--error)]" role="alert">
                $(error.get())
            </p>
            <div class="mt-4 flex items-center gap-3">
                <button
                    type="button"
                    class=(format!(
                        "{} bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)] disabled:opacity-40",
                        super::BUTTON,
                    ))
                    (save)
                >
                    $(if saving.get() { "Saving…" } else { "Save changes" })
                </button>
                <span
                    class="text-body-sm text-[var(--success)]"
                    aria-live="polite"
                    :hidden=$(!saved.get())
                >
                    "✓ Saved"
                </span>
            </div>
        </section>
    }.boxed()
}
