//! Joined member identities, active-human picker, role changes and inline recovery.
use super::super::{context, icons, session};
use super::{
    management_controls::{self, Pending},
    management_model::{Command, Continuation},
    select::{self, OptionRow},
};
use crate::{
    db::models::{MemberWithUser, Role},
    services::project_overview::OverviewReads,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};
fn roles() -> Vec<OptionRow> {
    [(0, "Viewer"), (1, "Maintainer"), (2, "Lead")]
        .into_iter()
        .map(|(value, label)| OptionRow {
            value: Some(value),
            label: label.into(),
            initials: String::new(),
            username: String::new(),
            admin: false,
            created_at: String::new(),
        })
        .collect()
}
pub(super) fn panel<'a>(
    cx: &'a Cx,
    reads: &OverviewReads,
    continuation: Option<&Continuation>,
) -> BoxView<'a> {
    let account = reads.user.id;
    let project = reads.project.id;
    let continuation = continuation.filter(|value| !matches!(value.command, Command::Lead { .. }));
    let initial_user = continuation.and_then(|value| match &value.command {
        Command::Add { user, .. } => Some(*user),
        _ => None,
    });
    let initial_role = continuation.map_or(0, |value| match &value.command {
        Command::Add { role, .. } => management_controls::role_number(role),
        _ => 0,
    });
    let label = continuation
        .and_then(|value| match value.command {
            Command::Role { user, .. } => Some(user),
            _ => None,
        })
        .and_then(|id| {
            reads
                .members
                .as_ref()
                .ok()
                .and_then(|members| members.iter().find(|member| member.user_id == id))
        })
        .map(|member| format!("@{}", member.username))
        .unwrap_or_default();
    let pending = Pending::new(cx, continuation, label);
    let revision = signal(cx, || 0_usize);
    let chosen_user = signal(cx, || initial_user);
    let chosen_role = signal(cx, || Some(initial_role));
    let error = signal(cx, String::new);
    let error_target = signal(cx, || 0_i64);
    let confirming = signal(cx, || 0_i64);
    let count = signal(cx, || reads.members.as_ref().map_or(0, Vec::len));
    let prompt =
        management_controls::prompt(cx, &pending, account, project, revision.clone(), false);
    let locked = pending.locked.clone();
    let busy = pending.busy.clone();
    let open = pending.open.clone();
    let frozen_kind = pending.kind_for_rows();
    let frozen_user = pending.user_for_rows();
    let frozen_role = pending.role_for_rows();
    view!{cx=><section class="native-overview__members"><div class="native-overview__heading">(icons::project_icon(cx,Some("lucide:UsersRound"),14))<h2>"Members"</h2><span :hidden=$(count.get()==0_usize)>$(count.get())</span></div>
        <div class="native-overview__member-card">
            native_overview_member_body(account:account,project:project,revision:$(revision.get()),owner_revision:revision.clone(),chosen_user:chosen_user,chosen_role:chosen_role,owner_error:error.clone(),owner_error_target:error_target.clone(),confirming:confirming,count:count,grant_state:(locked,busy,open,frozen_kind,frozen_user,frozen_role,pending.automatic_note.clone(),pending.error.clone(),pending.previous_for_rows(),pending.label_for_rows()))
            (prompt)
            <p role="alert" class="native-overview__member-error" :hidden=$(if error.get().is_empty(){true}else{error_target.get()!=0_i64})>$(error.get())</p>
        </div>
    </section>}.boxed()
}
use shards::native_overview_member_body;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat emits shard handlers with an extra context argument and drops function lint attributes"
)]
mod shards {
    use super::*;

    #[shard("/__native_overview/member_body")]
    pub(super) async fn native_overview_member_body(
        cx: &Cx,
        account: i64,
        project: i64,
        revision: usize,
        owner_revision: Signal<usize>,
        chosen_user: Signal<Option<i64>>,
        chosen_role: Signal<Option<i64>>,
        owner_error: Signal<String>,
        owner_error_target: Signal<i64>,
        confirming: Signal<i64>,
        count: Signal<usize>,
        grant_state: management_controls::GrantSignals,
    ) -> topcoat::Result<impl View> {
        let (
            locked,
            busy,
            pending_open,
            pending_kind,
            pending_user,
            pending_role,
            pending_note,
            pending_error,
            pending_previous,
            pending_label,
        ) = grant_state;
        let _ = revision;
        let caller = session::read(cx, context::caller(cx))?;
        let user = session::read(cx, crate::api::require_user(&caller.identity))?;
        if user.id != account {
            return Err(crate::error::LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )
            .into());
        }
        let members =
            crate::services::project_members::list(context::db(cx), &caller.identity, project)?;
        let can_manage = user.is_admin
            || members
                .iter()
                .any(|member| member.user_id == account && member.role == Role::Lead);
        let roster = if can_manage {
            crate::services::project_form::list_leads(context::db(cx), &caller.identity)?
        } else {
            Vec::new()
        };
        let eligible = roster
            .into_iter()
            .filter(|person| {
                person.is_active && !members.iter().any(|member| member.user_id == person.id)
            })
            .map(|person| OptionRow {
                value: Some(person.id),
                label: if person.display_name.is_empty() {
                    person.username.clone()
                } else {
                    person.display_name
                },
                initials: String::new(),
                username: person.username,
                admin: false,
                created_at: person.created_at,
            })
            .collect::<Vec<_>>();
        let mut options = vec![OptionRow::empty(if eligible.is_empty() {
            "No one left to add"
        } else {
            "Choose a person…"
        })];
        options.extend(eligible);
        let pending = Pending::from_rows(
            cx,
            pending_open,
            locked.clone(),
            busy.clone(),
            pending_kind,
            pending_user,
            pending_role,
            pending_previous,
            pending_label,
            pending_note,
            pending_error,
        );
        let add = management_controls::attempt(
            cx,
            &pending,
            account,
            project,
            "member_add",
            chosen_user.clone(),
            chosen_role.clone(),
            String::new(),
            String::new(),
            None,
            owner_revision.clone(),
            owner_error.clone(),
            Some(owner_error_target.clone()),
            "click",
        );
        let member_count = members.len();
        Ok(
            view! {cx=><div @mount=$(|_event:Event|{count.set(member_count);confirming.set(0_i64);})>
                if can_manage{<div class="native-overview__member-add">(select::select(cx,format!("native-overview-member-person-{project}"),options,chosen_user.clone(),locked.clone()))(select::select(cx,format!("native-overview-member-role-{project}"),roles(),chosen_role.clone(),locked.clone()))<button type="button" class="native-overview__success" :disabled=$(if locked.get(){true}else{chosen_user.get().is_none()}) (add)>(icons::project_icon(cx,Some("lucide:UserPlus"),14))$(if busy.get(){"Adding…"}else{"Add"})</button></div>}
                if members.is_empty(){<p class="native-overview__empty">"No members yet."</p>}
                else{for member in members{(row(cx,account,project,&member,can_manage,&pending,owner_revision.clone(),owner_error.clone(),owner_error_target.clone(),confirming.clone()))}}
                if !can_manage{<p class="native-overview__member-readonly">"Read-only — only a project lead can add, change, or remove members."</p>}
            </div>},
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn row<'a>(
    cx: &'a Cx,
    account: i64,
    project: i64,
    member: &MemberWithUser,
    can_manage: bool,
    pending: &Pending,
    revision: Signal<usize>,
    error: Signal<String>,
    error_target: Signal<i64>,
    confirming: Signal<i64>,
) -> BoxView<'a> {
    let user = member.user_id;
    let display = if member.display_name.is_empty() {
        member.username.clone()
    } else {
        member.display_name.clone()
    };
    let label = format!("@{}", member.username);
    let previous = member.role.as_str().to_owned();
    let number = management_controls::role_number(&previous);
    let selected = signal(cx, || Some(number));
    let target = signal(cx, || Some(user));
    let locked = pending.locked.clone();
    let busy = pending.busy.clone();
    let change = management_controls::attempt(
        cx,
        pending,
        account,
        project,
        "member_role",
        target.clone(),
        selected.clone(),
        previous,
        label.clone(),
        Some(number),
        revision.clone(),
        error.clone(),
        Some(error_target.clone()),
        "native-overview-selection",
    );
    let remove = management_controls::attempt(
        cx,
        pending,
        account,
        project,
        "member_remove",
        target,
        signal(cx, || Some(number)),
        String::new(),
        label.clone(),
        None,
        revision,
        error.clone(),
        Some(error_target.clone()),
        "click",
    );
    let badge = match member.role {
        Role::Lead => "lead",
        Role::Maintainer => "maintainer",
        Role::Viewer => "viewer",
    };
    let role_label = match member.role {
        Role::Lead => "Lead",
        Role::Maintainer => "Maintainer",
        Role::Viewer => "Viewer",
    };
    let since = super::dates::absolute(cx, &member.created_at);
    view!{cx=><div class="native-overview__member-row"><span class="native-overview__member-avatar">(select::initials(&display))</span><div class="native-overview__member-name"><p>(display.clone())if user==account{<span>" (you)"</span>}</p><p>(label)</p></div>
        if can_manage{<div class="native-overview__member-role" data-role=(badge) (change)>(select::select(cx,format!("native-overview-member-{project}-{user}"),roles(),selected,locked.clone()))</div>}
        else{<span class="native-overview__member-badge" data-role=(badge)>(role_label)</span>}
        <span class="native-overview__member-since">(since)</span>
        if can_manage{<button type="button" aria-label=(format!("Remove {display}")) class="native-overview__member-trash" :hidden=$(confirming.get()==user) :disabled=$(locked.get()) @click=$(|_event:Event|confirming.set(user))>(icons::project_icon(cx,Some("lucide:Trash2"),14))</button><div class="native-overview__member-remove" :hidden=$(confirming.get()!=user)><button type="button" class="native-overview__destructive" :disabled=$(busy.get()) (remove)>$(if busy.get(){"…"}else{"Remove"})</button><button type="button" @click=$(|_event:Event|confirming.set(0_i64))>"Cancel"</button></div>}
    </div><p class="native-overview__member-row-error" role="alert" :hidden=$(if error.get().is_empty(){true}else{error_target.get()!=user})>$(error.get())</p>}.boxed()
}
