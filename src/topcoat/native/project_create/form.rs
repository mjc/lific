//! Complete Rust runtime form state, frozen pending submission, and recovery UI.

use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::super::super::runtime::{string::StrUnicodeExt, whitespace::StrEcmaTrimExt};
use super::{
    actions::{confirm, create},
    model::Draft,
    select::{self, OptionRow},
};

#[derive(Clone)]
struct Form {
    account: i64,
    name: Signal<String>,
    identifier: Signal<String>,
    touched: Signal<bool>,
    description: Signal<String>,
    emoji: Signal<String>,
    lead: Signal<Option<i64>>,
    group: Signal<Option<i64>>,
    saving: Signal<bool>,
    error: Signal<String>,
    pending: Signal<bool>,
    password: Signal<String>,
    verifying: Signal<bool>,
    confirmation_error: Signal<String>,
    auto_note: Signal<String>,
    sent_name: Signal<String>,
    sent_identifier: Signal<String>,
    sent_touched: Signal<bool>,
    sent_description: Signal<String>,
    sent_emoji: Signal<String>,
    sent_lead: Signal<Option<i64>>,
    sent_group: Signal<Option<i64>>,
    input_index: Signal<usize>,
    input_count: Signal<usize>,
}

impl Form {
    fn new(cx: &Cx, account: i64, draft: Draft) -> Self {
        Self {
            account,
            name: signal(cx, || draft.name.clone()),
            identifier: signal(cx, || draft.identifier.clone()),
            touched: signal(cx, || draft.identifier_touched),
            description: signal(cx, || draft.description.clone()),
            emoji: signal(cx, || draft.emoji.clone()),
            lead: signal(cx, || draft.lead),
            group: signal(cx, || draft.group),
            saving: signal(cx, || false),
            error: signal(cx, || draft.error.clone()),
            pending: signal(cx, || false),
            password: signal(cx, String::new),
            verifying: signal(cx, || false),
            confirmation_error: signal(cx, String::new),
            auto_note: signal(cx, String::new),
            sent_name: signal(cx, String::new),
            sent_identifier: signal(cx, String::new),
            sent_touched: signal(cx, || false),
            sent_description: signal(cx, String::new),
            sent_emoji: signal(cx, String::new),
            sent_lead: signal(cx, || None::<i64>),
            sent_group: signal(cx, || None::<i64>),
            input_index: signal(cx, || 0_usize),
            input_count: signal(cx, || 0_usize),
        }
    }
}

fn command(cx: &Cx, state: &Form, confirmation: bool, keyboard: bool) -> Attributes {
    let account = state.account;
    let name = state.name.clone();
    let identifier = state.identifier.clone();
    let touched = state.touched.clone();
    let description = state.description.clone();
    let emoji = state.emoji.clone();
    let lead = state.lead.clone();
    let group = state.group.clone();
    let saving = state.saving.clone();
    let error = state.error.clone();
    let pending = state.pending.clone();
    let password = state.password.clone();
    let verifying = state.verifying.clone();
    let confirmation_error = state.confirmation_error.clone();
    let auto_note = state.auto_note.clone();
    let sent_name = state.sent_name.clone();
    let sent_identifier = state.sent_identifier.clone();
    let sent_touched = state.sent_touched.clone();
    let sent_description = state.sent_description.clone();
    let sent_emoji = state.sent_emoji.clone();
    let sent_lead = state.sent_lead.clone();
    let sent_group = state.sent_group.clone();
    let failed_saving = saving.clone();
    let failed_verifying = verifying.clone();
    let failed_password = password.clone();
    let failed_error = error.clone();
    let failed_confirmation_error = confirmation_error.clone();
    let handler = expr!(|event: Event| {
        let accepted_key = if keyboard { event.key == "Enter" } else { true };
        if accepted_key {
            event.prevent_default();
            let accepted = if confirmation {
                if pending.get() {
                    if verifying.get() {
                        false
                    } else {
                        !password.get().is_empty()
                    }
                } else {
                    false
                }
            } else {
                if saving.get() {
                    false
                } else if pending.get() {
                    false
                } else if name.get().trim_ecmascript().is_empty() {
                    false
                } else {
                    !identifier.get().trim_ecmascript().is_empty()
                }
            };
            if accepted {
                if !confirmation {
                    sent_name.set(name.get());
                    sent_identifier.set(identifier.get());
                    sent_touched.set(touched.get());
                    sent_description.set(description.get());
                    sent_emoji.set(emoji.get());
                    sent_lead.set(lead.get());
                    sent_group.set(group.get());
                    error.set("".to_owned());
                    auto_note.set("".to_owned());
                } else {
                    verifying.set(true);
                    confirmation_error.set("".to_owned());
                }
                saving.set(true);
                let supplied_password = password.get();
                let _failed = || {
                    failed_saving.set(false);
                    failed_verifying.set(false);
                    if confirmation {
                        failed_password.set("".to_owned());
                        failed_confirmation_error.set(
                            "Unable to verify. Your draft is still here. Try again.".to_owned(),
                        );
                    } else {
                        failed_error.set(
                            "Unable to create the project. Your draft is still here. Try again."
                                .to_owned(),
                        );
                    }
                };
                let _run = async || {
                    let result = if confirmation {
                        confirm(
                            account,
                            sent_name.get(),
                            sent_identifier.get(),
                            sent_touched.get(),
                            sent_description.get(),
                            sent_emoji.get(),
                            sent_lead.get(),
                            sent_group.get(),
                            supplied_password,
                        )
                        .await
                    } else {
                        create(
                            account,
                            sent_name.get(),
                            sent_identifier.get(),
                            sent_touched.get(),
                            sent_description.get(),
                            sent_emoji.get(),
                            sent_lead.get(),
                            sent_group.get(),
                        )
                        .await
                    };
                    saving.set(false);
                    verifying.set(false);
                    if confirmation {
                        password.set("".to_owned());
                    }
                    if if result.0 == "created" {
                        true
                    } else {
                        result.0 == "resume"
                    } {
                        let _destination = result.1;
                        raw!("window.location.assign(${_destination}.toString());", ());
                    } else {
                        if result.0 == "reauth" {
                            pending.set(true);
                            confirmation_error.set("".to_owned());
                            auto_note.set(result.2);
                            raw!(
                                "requestAnimationFrame(() => document.getElementById('native-project-confirm-password')?.focus());",
                                ()
                            );
                        } else {
                            if result.0 == "confirmation_failed" {
                                confirmation_error.set(result.1);
                            } else {
                                pending.set(false);
                                error.set(result.1);
                            }
                        }
                    }
                };
                // Generic Promise adaptation; Rust owns completion and failure.
                raw!(
                    "Promise.resolve().then(() => ${_run}()).catch(() => ${_failed}());",
                    ()
                );
            }
        }
    });
    let event = if keyboard {
        "keydown"
    } else if confirmation {
        "click"
    } else {
        "submit"
    };
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        format!("data-topcoat-on:{event}"),
        handler.into_evaluated_and_js().1,
    );
    attributes
}

pub(super) fn views<'a>(
    cx: &'a Cx,
    account: i64,
    draft: Draft,
    leads: Vec<OptionRow>,
    groups: Vec<OptionRow>,
    can_import: bool,
) -> (BoxView<'a>, BoxView<'a>) {
    let state = Form::new(cx, account, draft);
    let content = content(cx, &state, leads, groups, can_import);
    let header = topbar(cx, &state);
    (content, header)
}

fn content<'a>(
    cx: &'a Cx,
    state: &Form,
    leads: Vec<OptionRow>,
    groups: Vec<OptionRow>,
    can_import: bool,
) -> BoxView<'a> {
    let name = state.name.clone();
    let identifier = state.identifier.clone();
    let touched = state.touched.clone();
    let description = state.description.clone();
    let pending = state.pending.clone();
    let password = state.password.clone();
    let verifying = state.verifying.clone();
    let confirmation_error = state.confirmation_error.clone();
    let auto_note = state.auto_note.clone();
    let saving = state.saving.clone();
    let input_index = state.input_index.clone();
    let input_count = state.input_count.clone();
    // Own signal handles and event attributes before the lazy view captures them.
    let lead = state.lead.clone();
    let group = state.group.clone();
    let emoji = state.emoji.clone();
    let submit = command(cx, state, false, false);
    let confirm_keyboard = command(cx, state, true, true);
    let confirm_button = command(cx, state, true, false);
    let show_group = groups.len() > 1;
    let placeholder = {
        use rand::Rng;
        const NAMES: [&str; 13] = [
            "Half-Life 3",
            "Star Citizen 2",
            "Portal 3",
            "Aperture Science",
            "Rewriting Rust in Rust",
            "Is It DNS?",
            "TODO: Name This Later",
            "Untitled Goose Project",
            "Regex for Dummys",
            "Shovelware Simulator",
            "Moon Base Alpha",
            "Sentient Spreadsheet",
            "Banana for Scale",
        ];
        NAMES[rand::thread_rng().gen_range(0..NAMES.len())]
    };
    view! { cx =>
        <div class="native-project-create-page">
            if can_import {
                <p class="native-project-create__import">"Moving a project from another Lific instance? "
                    <a href=(super::super::transport::mounted_url(cx,"/projects/import"))>"Import a project archive"</a>
                </p>
            }
            <form id="native-project-create-form" class="native-project-create" (submit)>
                <div class="native-project-create__field">
                    <label for="project-name">"Name"</label>
                    <input id="project-name" type="text" autofocus="autofocus" placeholder=(placeholder) :value=$(name.get())
                        @input=$(|event: Event| {
                            let value = event.target.value; name.set(value.clone());
                            if !touched.get() {
                                if !value.is_empty() {
                                    let scalars = value.to_uppercase().unicode_scalars(0_usize);
                                    input_index.set(0_usize); input_count.set(0_usize);
                                    identifier.set("".to_owned());
                                    while if input_index.get() < scalars.len() { input_count.get() < 5_usize } else { false } {
                                        let scalar = scalars.index(input_index.get()).to_owned();
                                        if if scalar >= "A" { if scalar <= "Z" { true } else if scalar >= "0" { scalar <= "9" } else { false } } else if scalar >= "0" { scalar <= "9" } else { false } {
                                            identifier.push_str(scalar); input_count.increment();
                                        }
                                        input_index.increment();
                                    };
                                };
                            };
                        })>
                </div>
                <div class="native-project-create__identity">
                    <div class="native-project-create__identifier">
                        <label for="project-id">"Identifier"</label>
                        <input id="project-id" type="text" maxlength="5" spellcheck="false" autocapitalize="characters" placeholder="PRO"
                            :value=$(identifier.get()) @input=$(|event: Event| {identifier.set(event.target.value); touched.set(true);})>
                        <p class="native-project-create__hint">"Issues become"</p>
                        <span class="native-project-create__preview">$(if identifier.get().trim_ecmascript().is_empty() {"PRO".to_owned()} else {identifier.get().trim_ecmascript().to_uppercase()}) "-1"</span>
                    </div>
                    <div class="native-project-create__lead">
                        <label for="native-project-lead-trigger">"Lead"</label>
                        (select::select(cx,"native-project-lead",leads,lead))
                    </div>
                    <div><label for="native-project-icon-trigger">"Icon"</label>(super::picker_controls::picker(cx,emoji))</div>
                </div>
                if show_group {
                    <div class="native-project-create__field native-project-create__group">
                        <label for="native-project-group-trigger">"Group"</label>
                        (select::select(cx,"native-project-group",groups,group))
                    </div>
                }
                <div class="native-project-create__field">
                    <div class="native-project-create__description-label">
                        <label for="project-desc">"Description"</label>
                        <span class="native-project-create__optional">"optional"</span>
                    </div>
                    <textarea id="project-desc" rows="3" placeholder="What is this project about?" :value=$(description.get()) @input=$(|event: Event| description.set(event.target.value))></textarea>
                </div>

            </form>
                <section class="native-project-create__reauth-wrap" :hidden=$(!pending.get()) aria-label="Verify your sign-in">
                    <div class="native-project-create__reauth">
                    <div class="native-project-create__reauth-copy">
                    (super::super::icons::project_icon(cx, Some("lucide:Lock"), 15))
                    <p>"Verify it's you to create this project with the selected lead. Granting another person access requires a recent sign-in."</p>
                    </div>
                    <p role="status" :hidden=$(auto_note.get().is_empty())>$(auto_note.get())</p>
                    <input id="native-project-confirm-password" type="password" autocomplete="current-password" placeholder="Current password"
                        :value=$(password.get()) @input=$(|event: Event| password.set(event.target.value)) (confirm_keyboard)>
                    <p role="alert" class="native-project-create__error" :hidden=$(confirmation_error.get().is_empty())>$(confirmation_error.get())</p>
                    <div class="native-project-create__confirmation-actions">
                        <button type="button" :disabled=$(if verifying.get() { true } else { password.get().is_empty() }) (confirm_button)>$(if verifying.get() {"Verifying..."} else {"Verify and create"})</button>
                        <button type="button" :disabled=$(verifying.get()) @click=$(|_event: Event| {
                            pending.set(false); password.set("".to_owned()); confirmation_error.set("".to_owned()); auto_note.set("".to_owned()); saving.set(false);
                        })>"Cancel"</button>
                    </div>
                    </div>
                </section>
        </div>
    }.boxed()
}

fn topbar<'a>(cx: &'a Cx, state: &Form) -> BoxView<'a> {
    let name = state.name.clone();
    let identifier = state.identifier.clone();
    let saving = state.saving.clone();
    let pending = state.pending.clone();
    let error = state.error.clone();
    let settings = super::super::transport::mounted_url(cx, "/settings");
    view! { cx =>
        <div class="native-project-create__topbar">
            <div class="native-project-create__breadcrumb">
                <a class="native-project-create__back" href=(settings.clone())>
                    (super::super::icons::project_icon(cx, Some("lucide:ArrowLeft"), 14)) "Back"
                </a>
                <span aria-hidden="true">"/"</span><span class="native-project-create__title">"New project"</span>
            </div>
            <div class="native-project-create__actions">
                <span role="alert" class="native-project-create__error" :hidden=$(error.get().is_empty()) :title=$(error.get())>$(error.get())</span>
                <a href=(settings)>"Cancel"</a>
                <button type="submit" form="native-project-create-form" :disabled=$(if saving.get() { true } else if pending.get() { true } else if name.get().trim_ecmascript().is_empty() { true } else { identifier.get().trim_ecmascript().is_empty() })>$(if saving.get() {"Creating..."} else {"Create project"})</button>
            </div>
        </div>
    }.boxed()
}
