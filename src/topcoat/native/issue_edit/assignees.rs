//! Native issue assignment control and optimistic write procedure.

use super::super::super::runtime::signal_vec::{SignalVecExt, VecPositionExt};
use super::super::{context, session};
use crate::{db::queries::assignees::IssueAssignee, error::LificError};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, Signal, expr, procedure, record, shard, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

#[record]
#[derive(Clone, Default)]
pub(crate) struct AssignmentRequest {
    pub account_id: i64,
    pub issue_id: i64,
    pub identifier: String,
    pub expected_seq: i64,
    pub names: Vec<String>,
}

#[record]
#[derive(Clone, Default)]
pub(crate) struct AssignmentPerson {
    pub user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
}

#[record]
#[derive(Clone)]
pub(crate) struct AssignmentReply {
    pub status: Result<String, String>,
    pub account_id: i64,
    pub issue_id: i64,
    pub seq: i64,
    pub needs_human: bool,
    pub people: Vec<AssignmentPerson>,
}

type MenuSignals = (
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
);

impl Default for AssignmentReply {
    fn default() -> Self {
        Self {
            status: Ok(String::new()),
            account_id: 0,
            issue_id: 0,
            seq: 0,
            needs_human: false,
            people: Vec::new(),
        }
    }
}

#[procedure("/__native_issue_edit/assign")]
pub(crate) async fn assign_issue(
    cx: &Cx,
    request: AssignmentRequest,
) -> topcoat::Result<AssignmentReply> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = match crate::api::require_user(&caller.identity) {
        Ok(user) => user,
        Err(error) => return session::read(cx, Err(error)),
    };
    if user.id != request.account_id {
        return Ok(failed(&request, "Your account changed. Reload this page."));
    }
    let db = context::db(cx);
    let issue =
        match crate::services::issues::resolve_issue(db, &caller.identity, &request.identifier) {
            Ok(issue) if issue.id == request.issue_id => issue,
            Ok(_) | Err(LificError::NotFound(_)) => return Ok(failed(&request, "not found")),
            Err(error @ LificError::Forbidden(_)) => {
                return Ok(failed(&request, error.client_message()));
            }
            Err(error) => return Err(error.into()),
        };
    let committed = match caller
        .scope(async {
            crate::services::issues::commit_issue_assignment(
                db,
                app_context::<crate::realtime::RealtimeHub>(cx),
                &caller.identity,
                issue.id,
                request.expected_seq,
                request.names.clone(),
            )
        })
        .await
    {
        Ok(committed) => committed,
        Err(error) => return Ok(failed(&request, error.client_message())),
    };
    let saved = committed.issue;
    let assignment = committed.assignment;
    Ok(AssignmentReply {
        status: Ok("saved".into()),
        account_id: user.id,
        issue_id: saved.id,
        seq: saved.seq,
        needs_human: assignment.needs_human,
        people: assignment.assignees.into_iter().map(person).collect(),
    })
}

fn person(person: IssueAssignee) -> AssignmentPerson {
    AssignmentPerson {
        user_id: person.user_id,
        username: person.username,
        display_name: person.display_name,
    }
}

fn failed(request: &AssignmentRequest, message: &str) -> AssignmentReply {
    AssignmentReply {
        status: Err(message.to_owned()),
        account_id: request.account_id,
        issue_id: request.issue_id,
        seq: request.expected_seq,
        needs_human: false,
        people: Vec::new(),
    }
}

pub(crate) fn field<'a>(
    cx: &'a Cx,
    metadata: &super::route::DocumentMetadata,
    issue_id: i64,
    expected_seq: i64,
    identifier: &str,
    open: Signal<bool>,
    menus: MenuSignals,
) -> BoxView<'a> {
    let names = assignment_names(&metadata.assignment);
    let label = describe(&names, &metadata.assignee_people);
    if !metadata.can_edit {
        return view! {
            cx =>
            <span
                class=(if names.is_empty() {
                    "native-issue-detail__empty-value"
                } else {
                    ""
                })
            >
                (label)
            </span>
        }
        .boxed();
    }

    let account_id = metadata.account_id;

    let (status_open, header_status_open, priority_open, module_open, labels_open) = menus;
    let people = metadata.assignee_people.clone();
    let has_search = people.len() >= 7;
    let selected = signal(cx, || {
        names
            .iter()
            .filter(|name| name.as_str() != "human")
            .cloned()
            .collect::<Vec<_>>()
    });
    let query = signal(cx, String::new);
    let options_query = query.clone();
    let options_selected = selected.clone();
    let options_identifier = identifier.to_owned();
    let options_account_id = account_id;
    let query_input = query.clone();
    let query_handler = expr!(|event: Event| query_input.set(event.target.value.to_owned()));
    let mut query_attrs = Attributes::with_capacity(1);
    query_attrs.insert(
        cx,
        "data-topcoat-on:input",
        query_handler.into_evaluated_and_js().1,
    );
    let options = view! {
        cx =>
        native_assignee_options(
            account_id: options_account_id,
            identifier: options_identifier,
            filter: $(options_query.get()),
            selected: options_selected
        )
    }
    .boxed();
    let pick_open = open.clone();
    let pick_selected = selected.clone();
    let seed_selection = names
        .iter()
        .filter(|name| name.as_str() != "human")
        .cloned()
        .collect::<Vec<_>>();
    let toggle = expr!(|event: Event| {
        event.prevent_default();
        event.stop_propagation();
        if !pick_open.get() {
            status_open.set(false);
            header_status_open.set(false);
            priority_open.set(false);
            module_open.set(false);
            labels_open.set(false);
            pick_selected.set(seed_selection.clone());
        }
        pick_open.set(!pick_open.get());
    });
    let mut toggle_attrs = Attributes::with_capacity(1);
    toggle_attrs.insert(
        cx,
        "data-topcoat-on:click",
        toggle.into_evaluated_and_js().1,
    );
    let clear_request = request(account_id, issue_id, expected_seq, identifier, Vec::new());
    let human_request = request(
        account_id,
        issue_id,
        expected_seq,
        identifier,
        vec!["human".into()],
    );
    let clear = choice_attrs(cx, clear_request, &open);
    let human = choice_attrs(cx, human_request, &open);
    let apply_base = request(account_id, issue_id, expected_seq, identifier, Vec::new());
    let apply_selected = selected;
    let apply_open = open.clone();
    let apply = expr!(|event: Event| {
        event.prevent_default();
        let _request = AssignmentRequest {
            names: apply_selected.get(),
            account_id: apply_base.account_id.clone(),
            issue_id: apply_base.issue_id.clone(),
            identifier: apply_base.identifier.clone(),
            expected_seq: apply_base.expected_seq.clone(),
        };
        let accepted = raw!(
            "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-issue-assignee-request',{detail:${_request},cancelable:true})))",
            false
        );
        if accepted {
            apply_open.set(false);
        }
    });
    let mut apply_attrs = Attributes::with_capacity(1);
    apply_attrs.insert(cx, "data-topcoat-on:click", apply.into_evaluated_and_js().1);
    let cancel_open = open.clone();
    let cancel = expr!(|event: Event| {
        event.prevent_default();
        event.stop_propagation();
        cancel_open.set(false);
    });
    let mut cancel_attrs = Attributes::with_capacity(1);
    cancel_attrs.insert(
        cx,
        "data-topcoat-on:click",
        cancel.into_evaluated_and_js().1,
    );
    let dismiss_open = open.clone();
    let dismiss = expr!(|_mount: Event| {
        let _outside = |inside: bool| {
            if !inside {
                dismiss_open.set(false);
            }
        };
        raw!(
            "window.addEventListener('click',event=>${_outside}(cx.hydrate(Boolean(event.target?.closest?.('[data-native-issue-assignees]')))),{signal:cx.abortSignal})",
            ()
        );
    });
    let mut mount_attrs = Attributes::with_capacity(1);
    mount_attrs.insert(
        cx,
        "data-topcoat-on:mount",
        dismiss.into_evaluated_and_js().1,
    );
    let issue_identifier = identifier.to_owned();
    view! {
        cx =>
        <div
            class="native-issue-detail__picker relative min-w-0"
            data-native-issue-assignees=""
            (mount_attrs)
        >
            <button
                type="button"
                class="inline-flex min-w-0 items-center gap-1.5 rounded px-1.5 py-1 text-left text-sm text-[var(--text)] hover:bg-[var(--bg-subtle)]"
                aria-haspopup="dialog"
                title="Change assignee"
                :aria-expanded=$(open.get())
                (toggle_attrs)
            >
                (label)
            </button>
            <div
                class="absolute left-0 top-full z-30 mt-1 w-64 rounded-lg border border-[var(--border)] bg-[var(--bg)] p-1 shadow-lg"
                role="dialog"
                aria-label=(format!("Assign {issue_identifier}"))
                :hidden=$(!open.get())
                data-native-issue-assignee-options=""
            >
                <button
                    type="button"
                    data-native-issue-assignee-option=""
                    class="flex w-full rounded px-2 py-1.5 text-left text-sm hover:bg-[var(--bg-subtle)]"
                    (clear)
                >
                    "Any agent"
                </button>
                <button
                    type="button"
                    data-native-issue-assignee-option="human"
                    class="flex w-full rounded px-2 py-1.5 text-left text-sm hover:bg-[var(--bg-subtle)]"
                    (human)
                >
                    "Any person"
                </button>
                <p
                    class="px-2 pt-3 pb-1 text-micro uppercase tracking-widest text-[var(--text-faint)]"
                >
                    "Specific people"
                </p>
                if has_search {
                    <input
                        type="text"
                        placeholder="Filter people…"
                        aria-label="Filter people"
                        :value=$(query.get())
                        class="mb-1 w-full rounded border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)]"
                        (query_attrs)
                    />
                }
                (options)
                <div
                    class="mt-2 flex justify-end gap-2 border-t border-[var(--border)] pt-2"
                >
                    <button
                        type="button"
                        data-native-issue-assignee-cancel=""
                        class="rounded px-2 py-1.5 text-sm text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
                        (cancel_attrs)
                    >
                        "Cancel"
                    </button>
                    <button
                        type="button"
                        class="rounded bg-[var(--btn-success)] px-2 py-1.5 text-sm font-medium text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]"
                        data-native-issue-assignee-apply=""
                        (apply_attrs)
                    >
                        "Apply"
                    </button>
                </div>
            </div>
        </div>
    }
    .boxed()
}

#[shard("/__native_issue_edit/assignee_options")]
async fn native_assignee_options(
    cx: &Cx,
    account_id: i64,
    identifier: String,
    filter: String,
    selected: Signal<Vec<String>>,
) -> topcoat::Result<impl topcoat::view::View> {
    let caller = session::read(cx, context::caller(cx))?;
    let metadata = super::route::metadata(cx, &identifier, &caller.identity)?;
    let data = if metadata.account_id == account_id && metadata.can_edit {
        metadata.assignee_people
    } else {
        Vec::new()
    };
    let filtered = filter_people(&data, &filter)
        .into_iter()
        .map(|person| person_option(cx, person, selected.clone()))
        .collect::<Vec<_>>();
    Ok(view! {
        cx =>
        for option in filtered {
            (option)
        }
    })
}

fn filter_people<'a>(data: &'a [IssueAssignee], query: &str) -> Vec<&'a IssueAssignee> {
    let query = query.trim().trim_start_matches('@').trim().to_lowercase();
    data.iter()
        .filter(|person| {
            query.is_empty()
                || person.username.to_lowercase().contains(&query)
                || person
                    .display_name
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase().contains(&query))
        })
        .collect()
}

fn person_option<'a>(
    cx: &'a Cx,
    person: &IssueAssignee,
    selected: Signal<Vec<String>>,
) -> BoxView<'a> {
    let username = person.username.clone();
    let display_name = person.display_name.clone().unwrap_or_default();
    let browser = super::super::browser::bindings();
    let selected_toggle = selected.clone();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            let index = selected_toggle.get().position(username.clone());
            if index.is_some() {
                selected_toggle.remove(index.unwrap());
            } else {
                selected_toggle.push(username.clone());
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    view! {
        cx =>
        <button
            type="button"
            role="checkbox"
            :aria-checked=$(selected.get().position(username.clone()).is_some())
            data-native-issue-assignee-option=(username.clone())
            :class=$(if selected.get().position(username.clone()).is_some() {
                "flex w-full rounded bg-[var(--accent-subtle)] px-2 py-1.5 text-left text-sm"
            } else {
                "flex w-full rounded px-2 py-1.5 text-left text-sm hover:bg-[var(--bg-subtle)]"
            })
            (attrs)
        >
            $(if display_name.is_empty() {
                username.clone()
            } else {
                display_name.clone()
            })
        </button>
    }
    .boxed()
}

fn choice_attrs(cx: &Cx, request: AssignmentRequest, open: &Signal<bool>) -> Attributes {
    let close = open.clone();
    let browser = super::super::browser::bindings();
    let handler = expr!(|event: Event| {
        if !browser.is_disposed() {
            event.prevent_default();
            event.stop_propagation();
            let _request = request.clone();
            let accepted = raw!(
                "cx.hydrate(!window.dispatchEvent(new CustomEvent('lific:native-issue-assignee-request',{detail:${_request},cancelable:true})))",
                false
            );
            if accepted {
                close.set(false);
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

fn request(
    account_id: i64,
    issue_id: i64,
    expected_seq: i64,
    identifier: &str,
    names: Vec<String>,
) -> AssignmentRequest {
    AssignmentRequest {
        account_id,
        issue_id,
        identifier: identifier.to_owned(),
        expected_seq,
        names,
    }
}

fn assignment_names(assignment: &crate::db::queries::assignees::Assignment) -> Vec<String> {
    if !assignment.needs_human {
        Vec::new()
    } else if assignment.assignees.is_empty() {
        vec!["human".into()]
    } else {
        assignment
            .assignees
            .iter()
            .map(|person| person.username.clone())
            .collect()
    }
}

fn describe(names: &[String], people: &[IssueAssignee]) -> String {
    if names.is_empty() {
        return "Any agent".into();
    }
    if names.len() == 1 && names[0] == "human" {
        return "Any person".into();
    }
    names
        .iter()
        .map(|name| {
            people
                .iter()
                .find(|person| person.username.eq_ignore_ascii_case(name))
                .and_then(|person| person.display_name.clone())
                .filter(|display| !display.is_empty())
                .unwrap_or_else(|| format!("@{name}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::filter_people;
    use crate::db::queries::assignees::IssueAssignee;

    #[test]
    fn assignee_search_matches_username_and_display_name_case_insensitively() {
        let people = vec![
            IssueAssignee {
                user_id: 1,
                username: "alexandra".into(),
                display_name: Some("Alexandra Example".into()),
            },
            IssueAssignee {
                user_id: 2,
                username: "bobby".into(),
                display_name: Some("Robert Smith".into()),
            },
            IssueAssignee {
                user_id: 3,
                username: "carol".into(),
                display_name: None,
            },
        ];

        assert_eq!(
            filter_people(&people, "LEX")
                .iter()
                .map(|person| person.username.as_str())
                .collect::<Vec<_>>(),
            vec!["alexandra"]
        );
        assert_eq!(
            filter_people(&people, "SMITH")
                .iter()
                .map(|person| person.username.as_str())
                .collect::<Vec<_>>(),
            vec!["bobby"]
        );
        assert_eq!(
            filter_people(&people, "@alex")
                .iter()
                .map(|person| person.username.as_str())
                .collect::<Vec<_>>(),
            vec!["alexandra"]
        );
        assert_eq!(filter_people(&people, "  ").len(), people.len());
    }
}
