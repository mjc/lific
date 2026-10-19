//! Deliberate lead/rekey/delete controls with fresh server authority.
use super::super::super::runtime::string::StrUnicodeExt;
use super::super::icons::UiIcon;
use super::super::{context, icons, session, transport};
use super::actions::save_field;
use super::{
    management_controls::{self, Pending},
    management_model::{Command, Continuation},
};
use crate::{error::LificError, realtime::RealtimeHub, services::project_overview::OverviewReads};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, procedure, shard, signal},
    view::{BoxView, View, ViewExt, view},
};
#[procedure("/__native_overview/delete_project")]
async fn delete_project(
    cx: &Cx,
    account: i64,
    project: i64,
    confirmation: String,
) -> topcoat::Result<Result<String, String>> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Ok(Err("Your account changed. Reload this page.".into()));
    }
    let result = caller
        .scope(async {
            crate::services::project_overview::delete_confirmed(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                project,
                &confirmation,
            )?;
            Ok(transport::mounted_url(cx, "/settings"))
        })
        .await;
    Ok(result.map_err(super::actions::error_message))
}
pub(super) fn panel<'a>(
    cx: &'a Cx,
    reads: &OverviewReads,
    continuation: Option<&Continuation>,
) -> BoxView<'a> {
    let account = reads.user.id;
    let project = reads.project.id;
    let identifier = reads.project.identifier.clone();
    let continuation = continuation.filter(|value| matches!(value.command, Command::Lead { .. }));
    let pending = Pending::new(cx, continuation, String::new());
    let lead_revision = signal(cx, || 0_usize);
    let lead_error = signal(cx, String::new);
    let expanded = signal(cx, || continuation.is_some());
    let rename = signal(cx, String::new);
    let renaming = signal(cx, || false);
    let rename_error = signal(cx, String::new);
    let delete_open = signal(cx, || false);
    let confirmation = signal(cx, String::new);
    let deleting = signal(cx, || false);
    let delete_error = signal(cx, String::new);
    let prefix = transport::mounted_url(cx, "/");
    let total = reads.counts.as_ref().map_or(0, |counts| counts.total);
    let locked = pending.locked.clone();
    let busy = pending.busy.clone();
    let pending_open = pending.open.clone();
    let prompt =
        management_controls::prompt(cx, &pending, account, project, lead_revision.clone(), true);
    let shard_lead_error = lead_error.clone();
    let lead_shard = view! {
        cx =>
        native_overview_danger_lead(
            account: account,
            project: project,
            revision: $(lead_revision.get()),
            owner_revision: lead_revision.clone(),
            owner_error: shard_lead_error,
            grant_state: (
                locked,
                busy,
                pending_open,
                pending.kind_for_rows(),
                pending.user_for_rows(),
                pending.role_for_rows(),
                pending.previous_for_rows(),
                pending.label_for_rows(),
                pending.automatic_note.clone(),
                pending.error.clone(),
            )
        )
    }
    .boxed();
    let failed_renaming = renaming.clone();
    let failed_rename_error = rename_error.clone();
    let failed_deleting = deleting.clone();
    let failed_delete_error = delete_error.clone();
    view!{
        cx =>
        <section class="native-overview__danger">
            <button
                type="button"
                class="native-overview__danger-toggle"
                :aria-expanded=$(expanded.get())
                @click=$(|_event: Event| expanded.set(!expanded.get()))
            >
                (icons::ui_icon(cx, UiIcon::Warning, 15))
                <span>"Danger zone"</span>
                <span
                    class="native-overview__danger-chevron"
                    :data-open=$(expanded.get())
                >
                    (icons::ui_icon(cx, UiIcon::Expand, 15))
                </span>
            </button>
            <div class="native-overview__danger-body" :hidden=$(!expanded.get())>
                (lead_shard)
                (prompt)
                <p role="alert" :hidden=$(lead_error.get().is_empty())>
                    $(lead_error.get())
                </p>
                <div class="native-overview__danger-divider"></div>
                <div>
                    <h3>"Change identifier"</h3>
                    <p>
                        "Re-keys every issue, page, and plan. Existing references to "
                        <code>(format!("{identifier}-NNN"))</code>
                        " written inside other issues/pages will no longer resolve. This cannot be undone automatically."
                    </p>
                    <div class="native-overview__danger-actions">
                        <input
                            class="native-overview__rekey-input"
                            aria-label="New project identifier"
                            placeholder=(identifier.clone())
                            :value=$(rename.get())
                            @input=$(|event: Event| rename.set(event.target.value))
                        />
                        <button
                            type="button"
                            class="native-overview__destructive"
                            :disabled=$(if renaming.get() {
                                true
                            } else if rename.get().trim().is_empty() {
                                true
                            } else {
                                rename.get().trim().to_uppercase() == identifier
                            })
                            @click=$(|_event: Event| {
                                if !renaming.get() {
                                    renaming.set(true);
                                    rename_error.set("".to_owned());
                                    let value = rename.get();
                                    let _failed = || {
                                        failed_renaming.set(false);
                                        failed_rename_error.set(
                                            "Rename did not complete. Try again.".to_owned(),
                                        );
                                    };
                                    let _rename = async || {
                                        let result = save_field(
                                            account,
                                            project,
                                            "identifier".to_owned(),
                                            value.trim().to_uppercase(),
                                        ).await;
                                        renaming.set(false);
                                        if result.0.is_ok() {
                                            let _changed = result.4;
                                            raw!(
                                                "cx.navigate(${prefix}.toString()+${_changed}.toString()+'/overview');",
                                                (),
                                            );
                                        } else {
                                            rename_error.set(result.0.unwrap_err());
                                        }
                                    };
                                    raw!(
                                        "Promise.resolve().then(()=>${_rename}()).catch(()=>${_failed}());",
                                        (),
                                    );
                                }
                            })
                        >
                            $(if renaming.get() { "Renaming…" } else { "Rename" })
                        </button>
                    </div>
                    <p role="alert" :hidden=$(rename_error.get().is_empty())>
                        $(rename_error.get())
                    </p>
                </div>
                <div class="native-overview__danger-divider"></div>
                <div>
                    <h3>"Delete project"</h3>
                    <button
                        type="button"
                        class="native-overview__delete-open"
                        :hidden=$(delete_open.get())
                        @click=$(|_event: Event| delete_open.set(true))
                    >
                        "Delete this project"
                    </button>
                    <div :hidden=$(!delete_open.get())>
                        <p>
                            "Permanently deletes the project and all "
                            <strong>(total)</strong>
                            (if total == 1 { " issue" } else { " issues" })
                            ", modules, labels, folders, pages, and plans. Type "
                            <strong class="native-overview__identifier-text">
                                (identifier.clone())
                            </strong>
                            " to confirm."
                        </p>
                        <div
                            class="native-overview__danger-actions native-overview__danger-actions--delete"
                        >
                            <input
                                aria-label="Confirm project identifier"
                                class="native-overview__delete-input"
                                placeholder=(identifier.clone())
                                :value=$(confirmation.get())
                                @input=$(|event: Event| confirmation.set(event.target.value))
                            />
                            <button
                                type="button"
                                class="native-overview__destructive"
                                :disabled=$(if deleting.get() {
                                    true
                                } else {
                                    confirmation.get() != identifier
                                })
                                @click=$(|_event: Event| {
                                    if !deleting.get() {
                                        deleting.set(true);
                                        delete_error.set("".to_owned());
                                        let value = confirmation.get();
                                        let _failed = || {
                                            failed_deleting.set(false);
                                            failed_delete_error.set(
                                                "Delete did not complete. Try again.".to_owned(),
                                            );
                                        };
                                        let _delete = async || {
                                            let result = delete_project(account, project, value).await;
                                            deleting.set(false);
                                            if result.is_ok() {
                                                let _destination = result.unwrap();
                                                raw!("cx.navigate(${_destination}.toString());", ());
                                            } else {
                                                delete_error.set(result.unwrap_err());
                                            }
                                        };
                                        raw!(
                                            "Promise.resolve().then(()=>${_delete}()).catch(()=>${_failed}());",
                                            (),
                                        );
                                    }
                                })
                            >
                                $(if deleting.get() {
                                    "Deleting…"
                                } else {
                                    "Delete permanently"
                                })
                            </button>
                            <button
                                type="button"
                                @click=$(|_event: Event| {
                                    delete_open.set(false);
                                    confirmation.set("".to_owned());
                                    delete_error.set("".to_owned());
                                })
                            >
                                "Cancel"
                            </button>
                        </div>
                        <p role="alert" :hidden=$(delete_error.get().is_empty())>
                            $(delete_error.get())
                        </p>
                    </div>
                </div>
            </div>
        </section>
    }.boxed()
}
use shards::native_overview_danger_lead;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat emits shard handlers with an extra context argument and drops function lint attributes"
)]
mod shards {
    use super::*;

    #[shard("/__native_overview/danger_lead")]
    pub(super) async fn native_overview_danger_lead(
        cx: &Cx,
        account: i64,
        project: i64,
        revision: usize,
        owner_revision: Signal<usize>,
        owner_error: Signal<String>,
        grant_state: management_controls::GrantSignals,
    ) -> topcoat::Result<impl View> {
        let (
            locked,
            busy,
            pending_open,
            pending_kind,
            pending_user,
            pending_role,
            pending_previous,
            pending_label,
            pending_note,
            pending_error,
        ) = grant_state;
        let _ = revision;
        let caller = session::read(cx, context::caller(cx))?;
        let user = session::read(cx, crate::api::require_user(&caller.identity))?;
        if user.id != account {
            return Err(
                LificError::Forbidden("Your account changed. Reload this page.".into()).into(),
            );
        }
        session::read(
            cx,
            crate::authz::require_role(
                context::db(cx),
                &caller.identity,
                project,
                crate::db::models::Role::Viewer,
            ),
        )?;
        let saved = {
            let conn = context::db(cx).read()?;
            crate::db::queries::get_project(&conn, project)?
        };
        let roster = crate::services::project_form::list_leads(context::db(cx), &caller.identity)?;
        let initial = if pending_open.get_untracked() {
            pending_user.get_untracked()
        } else {
            saved.lead_user_id
        };
        let chosen = signal(cx, || initial);
        let mut values = vec![None];
        let mut options = vec![(None, "No lead".to_owned())];
        for person in roster {
            values.push(Some(person.id));
            options.push((
                Some(person.id),
                if person.display_name.is_empty() {
                    person.username
                } else {
                    person.display_name
                },
            ));
        }
        let pending = Pending::from_rows(
            cx,
            pending_open,
            locked.clone(),
            busy,
            pending_kind,
            pending_user,
            pending_role,
            pending_previous,
            pending_label,
            pending_note,
            pending_error,
        );
        let change = management_controls::attempt(
            cx,
            &pending,
            account,
            project,
            "lead",
            chosen.clone(),
            signal(cx, || None::<i64>),
            saved
                .lead_user_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            String::new(),
            saved.lead_user_id,
            owner_revision,
            owner_error,
            None,
            "native-overview-selection",
        );
        let index_width = usize::BITS;
        Ok(view! {
            cx =>
            <div class="native-overview__danger-lead">
                <div>
                    <h3>"Project lead"</h3>
                    <p>"Who owns this project."</p>
                </div>
                <select
                    aria-label="Project lead"
                    :disabled=$(locked.get())
                    @change=$(|_event: Event| {
                        let index = raw!(
                            "cx.hydrate({t:'usize',bits:Number(${index_width}.toString()),v:String(${_event}.target.selectedIndex)})",
                            0_usize,
                        );
                        chosen.set(values.index(index).clone());
                        raw!(
                            "${_event}.target.dispatchEvent(new Event('native-overview-selection',{bubbles:true}));",
                            (),
                        );
                    })
                    (change)
                >
                    for (index, (value, label)) in options.into_iter().enumerate() {
                        <option value=(index.to_string()) selected=(value == initial)>
                            (label)
                        </option>
                    }
                </select>
            </div>
        })
    }
}
