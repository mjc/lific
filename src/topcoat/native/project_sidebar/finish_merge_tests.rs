use super::super::model::{Group, Project};
use super::*;
fn catalog(owner: i64, generation: u64) -> Catalog {
    Catalog {
        owner,
        generation,
        projects: vec![
            Project {
                id: 1,
                identifier: "ONE".into(),
                name: "One".into(),
                emoji: None,
            },
            Project {
                id: 2,
                identifier: "TWO".into(),
                name: "Two".into(),
                emoji: None,
            },
        ],
        groups: vec![Group {
            id: 11,
            name: "Work".into(),
            project_ids: vec![1, 2],
        }],
    }
}
fn save(state: &mut State, name: &str) -> Write {
    if state.edit.is_none() {
        assert!(state.begin_edit(
            EditTarget::Existing(11),
            "native-sidebar-group-actions-11-phone".into()
        ));
    }
    state.draft(name.into());
    let (target, name) = state.begin_save().unwrap();
    Write::SaveGroup {
        token: state.save_token().unwrap(),
        target,
        name,
    }
}
fn old_outcome() -> Applied {
    let mut c = catalog(7, 90);
    c.projects[0].name = "Old returned title".into();
    Applied {
        catalog: Some(c),
        groups_ready: false,
        error: Some("Old write error".into()),
        warning: "Old write warning".into(),
    }
}
fn unchanged(state: &mut State, write: Write) {
    unchanged_with(state, write, old_outcome());
}
fn unchanged_with(state: &mut State, write: Write, outcome: Applied) {
    let before = encode(state).unwrap();
    let focus = merge(state, write, outcome).unwrap();
    assert!(
        focus.is_empty(),
        "Rejected completion cannot restore old focus"
    );
    assert_eq!(
        encode(state).unwrap(),
        before,
        "Rejected completion must not change catalog, flags, editor, current errors, or warnings"
    );
}
fn save_case(case: &str) {
    let mut state = State::new(catalog(7, 1));
    let old = save(&mut state, "Old draft");
    let token = state.save_token().unwrap();
    match case {
        "success" | "error" => {
            state.finish_save_token(token, Err("First attempt rejected".into()));
            let current = save(&mut state, "Current draft");
            let Write::SaveGroup { token, .. } = current else {
                unreachable!()
            };
            state.finish_save_token(
                token,
                if case == "success" {
                    Ok(())
                } else {
                    Err("Current editor error".into())
                },
            );
        }
        "account" => {
            state.reset_owner(catalog(8, 3));
            save(&mut state, "New account draft");
        }
        "disconnect" => state.disconnect(),
        _ => unreachable!(),
    }
    if case != "success" {
        state.error = "Current sidebar error".into();
    }
    unchanged(&mut state, old);
}
fn order_case(case: &str) {
    let mut state = State::new(catalog(7, 1));
    let token = state.begin_order(Some(&[2, 1]), None).unwrap();
    let old = Write::OrderProjects {
        token,
        ids: vec![2, 1],
    };
    match case {
        "success" | "error" => {
            assert!(state.finish_order(token, Ok(catalog(7, 2))));
            let current = state.begin_order(Some(&[2, 1]), None).unwrap();
            let mut next = catalog(7, 3);
            next.projects.reverse();
            assert!(state.finish_order(
                current,
                if case == "success" {
                    Ok(next)
                } else {
                    Err("Current order error".into())
                }
            ));
        }
        "account" => {
            state.reset_owner(catalog(8, 3));
            assert!(state.begin_order(Some(&[2, 1]), None).is_some());
            state.error = "New account error".into();
        }
        "disconnect" => {
            state.disconnect();
            state.error = "Current disconnected error".into();
        }
        _ => unreachable!(),
    }
    unchanged(&mut state, old);
}
#[test]
fn old_save_warning_after_newer_success_is_wholly_ignored() {
    save_case("success");
}
#[test]
fn old_save_warning_after_newer_error_is_wholly_ignored() {
    save_case("error");
}
#[test]
fn old_save_warning_after_account_reset_is_wholly_ignored() {
    save_case("account");
}
#[test]
fn old_save_warning_after_disconnect_is_wholly_ignored() {
    save_case("disconnect");
}
#[test]
fn old_order_warning_after_newer_success_is_wholly_ignored() {
    order_case("success");
}
#[test]
fn old_order_warning_after_newer_error_is_wholly_ignored() {
    order_case("error");
}
#[test]
fn old_order_warning_after_account_reset_is_wholly_ignored() {
    order_case("account");
}
#[test]
fn old_order_warning_after_disconnect_is_wholly_ignored() {
    order_case("disconnect");
}
#[test]
fn old_refresh_error_and_warning_after_newer_success_are_wholly_ignored() {
    let mut state = State::new(catalog(7, 1));
    let old = state.begin_refresh().unwrap();
    let current = state.begin_refresh().unwrap();
    assert!(state.complete_refresh(current, catalog(7, 2)));
    unchanged(&mut state, Write::Refresh { token: old });
}
#[test]
fn old_refresh_metadata_after_group_save_admission_is_wholly_ignored() {
    let mut state = State::new(catalog(7, 1));
    let old = state.begin_refresh().unwrap();
    save(&mut state, "Current draft");
    state.error = "Keep current error".into();
    let mut outcome = old_outcome();
    outcome.catalog = None;
    outcome.warning.clear();
    unchanged_with(&mut state, Write::Refresh { token: old }, outcome);
}
#[test]
fn current_group_commit_keeps_its_partial_warning_and_trigger_focus() {
    let mut state = State::new(catalog(7, 1));
    let current = save(&mut state, "Current draft");
    let focus = merge(
        &mut state,
        current,
        Applied {
            catalog: Some(catalog(7, 2)),
            groups_ready: true,
            error: None,
            warning: "Group created but project could not be moved".into(),
        },
    )
    .unwrap();
    assert_eq!(focus, "native-sidebar-group-actions-11-phone");
    assert!(state.edit.is_none());
    assert_eq!(state.catalog.generation, 2);
    assert!(state.groups_ready);
    assert_eq!(state.error, "Group created but project could not be moved");
}
