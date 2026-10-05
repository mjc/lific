use super::*;
fn project(id: i64, identifier: &str) -> Project {
    Project {
        id,
        identifier: identifier.into(),
        name: identifier.into(),
        emoji: None,
    }
}
fn row(id: i64) -> Row {
    Row {
        href: format!("/LIF/issues/LIF-{id}"),
        label: format!("Issue {id}"),
        identifier: Some(format!("LIF-{id}")),
    }
}
fn ready(section: Section) -> State {
    let mut state = State::new(1, false);
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(section), true)
        .unwrap();
    assert!(state.finish(token, Ok(vec![row(1)]), None).is_some());
    state
}
#[test]
fn same_project_refresh_keeps_prior_rows_and_disclosure_including_return_from_another_section() {
    let mut state = ready(Section::Issues);
    state.open = true;
    assert!(
        state
            .begin(1, Some(&project(7, "LIF")), None, true)
            .is_none()
    );
    assert!(!state.visible);
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert_eq!(state.rows(), [row(1)]);
    assert!(state.open);
    assert!(state.loading);
    state.finish(token, Ok(vec![row(2)]), None).unwrap();
    assert_eq!(state.rows(), [row(2)]);
    assert!(!state.loading);
    for section in [Section::Modules, Section::Pages, Section::Plans] {
        let token = state
            .begin(1, Some(&project(7, "LIF")), Some(section), true)
            .unwrap();
        state
            .finish(token, Ok(vec![row(section.index() as i64 + 10)]), None)
            .unwrap();
    }
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    state
        .finish(
            token,
            Err(ReadFailure {
                access: false,
                message: "Offline".into(),
            }),
            None,
        )
        .unwrap();
    assert_eq!(state.rows(), [row(2)]);
    for section in [Section::Modules, Section::Pages, Section::Plans] {
        state.begin(1, Some(&project(7, "LIF")), Some(section), true);
        assert_eq!(state.rows(), [row(section.index() as i64 + 10)]);
    }
}
#[test]
fn activation_captures_project_identity_before_awaiting_responses() {
    let mut mutable = project(7, "LIF");
    let mut state = State::new(1, false);
    let token = state
        .begin(1, Some(&mutable), Some(Section::Issues), true)
        .unwrap();
    mutable.id = 42;
    mutable.identifier = "OTHER".into();
    state.finish(token, Ok(vec![row(1)]), None).unwrap();
    assert_eq!(state.project, Some((7, "LIF".into())));
    assert_eq!(state.rows()[0].href, "/LIF/issues/LIF-1");
}
#[test]
fn switching_projects_clears_immediately_and_older_detail_route_replies_cannot_overwrite_rows() {
    let mut state = ready(Section::Issues);
    let older = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    let newer = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    state.finish(newer, Ok(vec![row(3)]), None).unwrap();
    assert!(state.finish(older, Ok(vec![row(2)]), None).is_none());
    assert_eq!(state.rows(), [row(3)]);
    let old_project = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    let other = state
        .begin(1, Some(&project(8, "OTHER")), Some(Section::Issues), true)
        .unwrap();
    assert!(state.rows().is_empty());
    assert!(state.finish(old_project, Ok(vec![row(4)]), None).is_none());
    assert!(state.rows().is_empty());
    let other_row = Row {
        href: "/OTHER/issues/OTHER-9".into(),
        label: "Other issue".into(),
        identifier: Some("OTHER-9".into()),
    };
    state
        .finish(other, Ok(vec![other_row.clone()]), None)
        .unwrap();
    assert_eq!(state.rows(), [other_row]);
}
#[test]
fn public_scope_and_account_invalidation_drop_private_links_and_reject_pending_replies() {
    let mut state = ready(Section::Issues);
    let pending = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert!(
        state
            .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), false)
            .is_none()
    );
    assert!(!state.visible);
    assert!(state.rows().is_empty());
    assert!(state.finish(pending, Ok(vec![row(2)]), None).is_none());
    assert!(state.rows().is_empty());
    let next = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    state.invalidate(2);
    assert!(state.finish(next, Ok(vec![row(3)]), None).is_none());
    assert!(!state.visible);
    assert!(state.rows().is_empty());
}
// These decision contracts retain non-DOM assertions. Their actual mounted
// link/focus/ARIA assertions also live in browser-assertions.cjs.
#[test]
fn mounted_recents_disclose_accessible_links_announce_refresh_preserve_focused_rows_and_clear_on_project_switch()
 {
    let mut state = ready(Section::Issues);
    state.cache[0] = vec![Row {
        href: "/LIF/issues/LIF-1".into(),
        label: "Keep <this> literal".into(),
        identifier: Some("LIF-1".into()),
    }];
    assert!(!state.open);
    assert_eq!(
        state.rows()[0].accessible_name(),
        "LIF-1: Keep <this> literal"
    );
    state.open = true;
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert!(state.loading);
    assert_eq!(state.status(), "Loading recent issues…");
    let rows = state.rows().to_vec();
    assert_eq!(
        state.finish(token, Ok(rows), Some("/LIF/issues/LIF-1")),
        Some(Focus::Keep("/LIF/issues/LIF-1".into()))
    );
    assert!(!state.loading);
    let pending = state
        .begin(1, Some(&project(8, "OTHER")), Some(Section::Issues), true)
        .unwrap();
    assert!(state.rows().is_empty());
    assert!(state.open);
    state.disconnect();
    assert!(state.finish(pending, Ok(vec![row(9)]), None).is_none());
    assert!(state.rows().is_empty());
}
#[test]
fn mounted_recents_return_focus_to_the_disclosure_when_refresh_removes_the_focused_row() {
    let mut state = ready(Section::Issues);
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert_eq!(
        state.finish(token, Ok(vec![row(2)]), Some("/LIF/issues/LIF-1")),
        Some(Focus::Disclosure)
    );
}
#[test]
fn mounted_public_and_anonymous_scopes_emit_neither_private_requests_nor_private_links() {
    let mut state = State::new(1, false);
    assert!(
        state
            .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), false)
            .is_none()
    );
    assert!(!state.visible);
    assert!(state.rows().is_empty());
    assert!(state.begin(0, None, Some(Section::Issues), true).is_none());
    assert!(!state.visible);
    assert!(state.rows().is_empty());
}
#[test]
fn disclosure_survives_native_document_navigation_without_storing_resource_rows() {
    let mut state = ready(Section::Issues);
    state.open = true;
    // Document persistence is only a boolean, never the serialized resource cache.
    let next = State::new(1, state.open);
    assert!(next.open);
    assert!(next.rows().is_empty());
    assert!(!next.loading);
}
#[test]
fn account_changes_discard_the_previous_catalog_and_accept_the_next_account_generation() {
    let mut state = ready(Section::Issues);
    let old = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    state.invalidate(2);
    assert!(!state.visible);
    assert!(state.rows().is_empty());
    let next = state
        .begin(2, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert!(state.finish(old, Ok(vec![row(1)]), None).is_none());
    state.finish(next, Ok(vec![row(2)]), None).unwrap();
    assert!(state.visible);
    assert_eq!(state.rows(), [row(2)]);
}
#[test]
fn transient_refresh_errors_keep_successful_rows_while_revoked_access_removes_them() {
    for access in [false, true] {
        let mut state = ready(Section::Issues);
        let token = state
            .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
            .unwrap();
        state
            .finish(
                token,
                Err(ReadFailure {
                    access,
                    message: "Could not load".into(),
                }),
                None,
            )
            .unwrap();
        assert!(!state.loading);
        assert_eq!(state.error.as_deref(), Some("Could not load"));
        assert_eq!(state.rows().len(), usize::from(!access));
    }
}
#[test]
fn a_partial_page_lifecycle_failure_preserves_the_last_complete_combined_result() {
    let mut state = ready(Section::Pages);
    let previous = state.rows().to_vec();
    let previous_identity = state.rows().as_ptr();
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Pages), true)
        .unwrap();
    let result = crate::services::project_recents::page_rows(
        "LIF",
        [
            Ok(vec![]),
            Err(ReadFailure {
                access: false,
                message: "Offline".into(),
            }),
            Ok(vec![]),
        ],
    );
    state.finish(token, result, None).unwrap();
    assert_eq!(state.rows(), previous);
    assert_eq!(state.rows().as_ptr(), previous_identity);
    assert_eq!(state.error.as_deref(), Some("Offline"));
}
#[test]
fn a_page_lifecycle_access_failure_takes_precedence_over_a_transient_sibling_failure() {
    let mut state = ready(Section::Pages);
    let token = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Pages), true)
        .unwrap();
    let result = crate::services::project_recents::page_rows(
        "LIF",
        [
            Err(ReadFailure {
                access: false,
                message: "Offline".into(),
            }),
            Err(ReadFailure {
                access: true,
                message: "Forbidden".into(),
            }),
            Ok(vec![]),
        ],
    );
    state.finish(token, result, None).unwrap();
    assert!(state.rows().is_empty());
    assert_eq!(state.error.as_deref(), Some("Forbidden"));
}
#[test]
fn stale_failures_cannot_clear_a_newer_success_or_end_its_loading_state() {
    let mut state = ready(Section::Issues);
    let old = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    let next = state
        .begin(1, Some(&project(7, "LIF")), Some(Section::Issues), true)
        .unwrap();
    assert!(
        state
            .finish(
                old,
                Err(ReadFailure {
                    access: true,
                    message: "Forbidden".into()
                }),
                None
            )
            .is_none()
    );
    assert!(state.loading);
    assert!(state.error.is_none());
    state.finish(next, Ok(vec![row(2)]), None).unwrap();
    assert_eq!(state.rows(), [row(2)]);
    assert!(state.error.is_none());
}
#[test]
fn native_detail_route_classification_never_selects_board_or_prefix_neighbors() {
    for (path, section) in [
        ("/LIF/issues", Some(Section::Issues)),
        ("/LIF/issues/LIF-1", Some(Section::Issues)),
        ("/LIF/pages/2", Some(Section::Pages)),
        ("/LIF/modules/3", Some(Section::Modules)),
        ("/LIF/plans/4", Some(Section::Plans)),
        ("/LIF/board", None),
        ("/LIFIC/issues", None),
        ("/settings", None),
    ] {
        assert_eq!(Section::for_path("LIF", path), section);
    }
}

#[test]
fn envelope_rejects_other_project_links_and_oversized_cache() {
    let mut state = State::new(1, false);
    state.project = Some((7, "LIF".into()));
    state.section = Some(Section::Issues);
    state.visible = true;
    state.cache[0] = vec![row(1)];
    assert!(state.valid_envelope());
    state.cache[0][0].href = "/HIDDEN/issues/HIDDEN-1".into();
    assert!(!state.valid_envelope());
    state.cache[0] = vec![row(1); 6];
    assert!(!state.valid_envelope());
}
