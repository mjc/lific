use super::super::super::runtime::whitespace::StrEcmaTrimExt;
use super::super::{account_profile, session};
use super::actions::{profile_session, save_profile};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(super) fn section(cx: &Cx, handles: account_profile::Handles) -> BoxView<'_> {
    view! { cx => native_settings_profile(handles: handles) }.boxed()
}

#[shard("/__native_settings/profile")]
async fn native_settings_profile(
    cx: &Cx,
    handles: account_profile::Handles,
) -> topcoat::Result<impl View> {
    session::read(cx, super::actions::same_account(cx, handles.0))?;
    Ok(render_section(cx, handles))
}

#[derive(Clone)]
struct Draft {
    name: Signal<String>,
    email: Signal<String>,
    saving: Signal<bool>,
    saved: Signal<bool>,
    saved_generation: Signal<usize>,
    error: Signal<String>,
}

fn save_attrs(cx: &Cx, handles: account_profile::Handles, draft: Draft) -> Attributes {
    let (account, canonical, revision) = handles;
    let Draft {
        name,
        email,
        saving,
        saved,
        saved_generation,
        error,
    } = draft;
    let checked_revision = revision.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        if !saving.get() {
            let baseline = canonical.get();
            let requested_name = name.get().trim_ecmascript().to_owned();
            let requested_email = email.get().trim_ecmascript().to_owned();
            let email_for_normalization = requested_email.clone();
            let normalized_email = raw!(
                "cx.hydrate(${email_for_normalization}.toString().toLowerCase())",
                email_for_normalization.to_lowercase()
            );
            let name_change = if requested_name != baseline.display_name {
                Some(requested_name)
            } else {
                None
            };
            let email_change = if normalized_email != baseline.email {
                Some(requested_email)
            } else {
                None
            };
            let changed = if name_change.is_some() {
                true
            } else {
                email_change.is_some()
            };
            if changed {
                let sent_revision = revision.get();
                saving.set(true);
                error.set("".to_owned());
                let _alive = || !raw!("cx.hydrate(cx.abortSignal.aborted)", false);
                let _current = || {
                    if raw!("${_alive}()", true) {
                        checked_revision.get() == sent_revision
                    } else {
                        false
                    }
                };
                let _finish = || {
                    if raw!("${_alive}()", true) {
                        saving.set(false);
                    }
                };
                let _failed = || {
                    raw!("${_finish}();", ());
                    if raw!("${_current}()", false) {
                        failed_error.set("Couldn't save your profile. Try again.".to_owned());
                    }
                };
                let _run = async || {
                    if !raw!("${_current}()", false) {
                        raw!("${_finish}();", ());
                        return;
                    }
                    let outcome = save_profile(account, name_change, email_change).await;
                    if raw!("${_current}()", false) {
                        let _verify = async || {
                            if !raw!("${_current}()", false) {
                                raw!("${_finish}();", ());
                                return;
                            }
                            let current_session = profile_session(account).await;
                            raw!("${_finish}();", ());
                            if raw!("${_current}()", false) {
                                if current_session.is_ok() {
                                    let session = current_session.unwrap();
                                    let unchanged_session = if session.is_some() {
                                        if outcome.session.is_some() {
                                            session.unwrap() == outcome.session.unwrap()
                                        } else {
                                            false
                                        }
                                    } else {
                                        outcome.session.is_none()
                                    };
                                    if unchanged_session {
                                        if outcome.profile.is_ok() {
                                            let profile = outcome.profile.unwrap();
                                            name.set(profile.display_name.clone());
                                            email.set(profile.email.clone());
                                            canonical.set(profile);
                                            revision.increment();
                                            saved.set(true);
                                            saved_generation.increment();
                                            let generation = saved_generation.get();
                                            let _expire = || {
                                                if !raw!(
                                                    "cx.hydrate(cx.abortSignal.aborted)",
                                                    false
                                                ) {
                                                    if saved_generation.get() == generation {
                                                        saved.set(false);
                                                    }
                                                }
                                            };
                                            raw!(
                                                "const cancel=()=>clearTimeout(timer); const timer=setTimeout(()=>{cx.abortSignal.removeEventListener('abort',cancel);${_expire}();},2000);cx.abortSignal.addEventListener('abort',cancel,{once:true});",
                                                ()
                                            );
                                        } else {
                                            error.set(outcome.profile.unwrap_err());
                                        }
                                    }
                                }
                            }
                        };
                        raw!(
                            "Promise.resolve().then(()=>${_verify}()).catch(()=>${_finish}());",
                            ()
                        );
                    } else {
                        raw!("${_finish}();", ());
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
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

fn render_section(cx: &Cx, handles: account_profile::Handles) -> BoxView<'_> {
    let (account, canonical, _) = handles.clone();
    let baseline = canonical.get_untracked();
    let username = baseline.username;
    let state_cx = cx.keyed((account, "profile"));
    let profile_name = signal(&state_cx, || baseline.display_name);
    let profile_email = signal(&state_cx, || baseline.email);
    let saving = signal(&state_cx, || false);
    let saved = signal(&state_cx, || false);
    let saved_generation = signal(&state_cx, || 0usize);
    let error = signal(&state_cx, String::new);
    let save = save_attrs(
        cx,
        handles,
        Draft {
            name: profile_name.clone(),
            email: profile_email.clone(),
            saving: saving.clone(),
            saved: saved.clone(),
            saved_generation,
            error: error.clone(),
        },
    );
    view! {
        cx =>
        <section data-native-profile="">
            <div class="flex max-w-[480px] flex-col gap-3.5">
                <label class="block">
                    <span
                        class="mb-1.5 block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                    >
                        "Display name"
                    </span>
                    <input
                        class=(super::INPUT)
                        data-native-profile-field="display_name"
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
                        data-native-profile-field="email"
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
                    data-native-profile-save=""
                    :disabled=$(if saving.get() {
                        true
                    } else {
                        let baseline = canonical.get();
                        let entered_email = profile_email.get();
                        let normalized_email = raw!(
                            "cx.hydrate(${entered_email}.toString().trim().toLowerCase())",
                            entered_email.trim().to_lowercase(),
                        );
                        if profile_name.get().trim_ecmascript().to_owned()
                            != baseline.display_name {
                            false
                        } else {
                            normalized_email == baseline.email
                        }
                    })
                    (save)
                >
                    $(if saving.get() { "Saving…" } else { "Save changes" })
                </button>
                <span
                    class="text-body-sm text-[var(--success)]"
                    aria-live="polite"
                    data-native-profile-saved=""
                    :hidden=$(!saved.get())
                >
                    "✓ Saved"
                </span>
            </div>
        </section>
    }.boxed()
}
