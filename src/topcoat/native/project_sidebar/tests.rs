use super::*;

#[tokio::test]
async fn destination_icons_preserve_main_glyphs() {
    use topcoat::view::ViewExt;
    let cx = topcoat::context::Cx::default();
    for (destination, glyph) in [
        (Destination::Overview, "LayoutDashboard"),
        (Destination::Issues, "List"),
        (Destination::Board, "LayoutGrid"),
        (Destination::Graph, "Waypoints"),
        (Destination::Modules, "Layers"),
        (Destination::Pages, "FileText"),
        (Destination::Files, "Paperclip"),
        (Destination::Plans, "ListChecks"),
        (Destination::Activity, "History"),
        (Destination::Insights, "TrendingUp"),
    ] {
        let (_, _, icon) = destination.row();
        let html = super::super::super::icons::ui_icon(&cx, icon, 14)
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains(&format!("data-icon=\"{glyph}\"")),
            "{destination:?}: {html}"
        );
    }
}

fn catalog(generation: u64) -> Catalog {
    Catalog {
        owner: 7,
        generation,
        projects: [1, 2, 3, 4, 5]
            .into_iter()
            .map(|id| Project {
                id,
                identifier: format!("P{id}"),
                name: format!("Project {id}"),
                emoji: None,
            })
            .collect(),
        groups: vec![
            Group {
                id: 11,
                name: "Work".into(),
                project_ids: vec![5, 2],
            },
            Group {
                id: 12,
                name: "Other".into(),
                project_ids: vec![3],
            },
        ],
    }
}
#[test]
fn project_rows_follow_catalog_project_order_within_groups_and_ungrouped_projects() {
    assert_eq!(
        catalog(1).rows(),
        [
            (Some(11), 2),
            (Some(11), 5),
            (Some(12), 3),
            (None, 1),
            (None, 4)
        ]
    );
}
#[test]
fn rendered_group_rows_follow_canonical_catalog_order() {
    let mut c = catalog(1);
    c.groups.reverse();
    assert_eq!(
        c.rows(),
        [
            (Some(12), 3),
            (Some(11), 2),
            (Some(11), 5),
            (None, 1),
            (None, 4)
        ]
    );
}
#[test]
fn deleting_group_leaves_projects_ungrouped_in_canonical_order() {
    let mut c = catalog(1);
    c.groups.remove(0);
    assert_eq!(c.siblings(None), [1, 2, 4, 5]);
    assert_eq!(c.project_ids(), [1, 2, 3, 4, 5]);
}
#[test]
fn group_reorder_keeps_all_project_catalog_positions() {
    let mut state = State::new(catalog(1));
    state.begin_order(None, Some(&[12, 11])).unwrap();
    assert_eq!(state.catalog.project_ids(), [1, 2, 3, 4, 5]);
    assert_eq!(state.catalog.group_ids(), [12, 11]);
}
#[test]
fn menu_move_swaps_only_siblings_and_preserves_group_assignment() {
    let c = catalog(1);
    assert_eq!(
        move_project(&c, 2, Direction::Down),
        Some(vec![1, 5, 3, 4, 2])
    );
    assert_eq!(
        move_project(&c, 5, Direction::Up),
        Some(vec![1, 5, 3, 4, 2])
    );
    assert_eq!(c.containing(2), Some(11));
    assert_eq!(move_project(&c, 2, Direction::Up), None);
    assert_eq!(move_project(&c, 3, Direction::Down), None);
    assert_eq!(move_group(&c, 12, Direction::Up), Some(vec![12, 11]));
}
#[test]
fn master_drag_anchors_ungrouped_neighbor_without_losing_outside_rows() {
    let c = catalog(1);
    assert_eq!(drag_order(&c, 4, &[4, 1]), Some(vec![4, 1, 2, 3, 5]));
    assert_eq!(drag_order(&c, 1, &[4, 1]), Some(vec![2, 3, 4, 1, 5]));
    assert_eq!(drag_order(&c, 2, &[4, 1]), None);
    assert_eq!(drag_order(&c, 1, &[1, 1]), None);
    assert_eq!(drag_order(&c, 1, &[4, 1, 99]), None);
}
#[test]
fn delayed_older_refresh_cannot_replace_newer_catalog() {
    let mut s = State::new(catalog(1));
    let old = s.begin_refresh().unwrap();
    let new = s.begin_refresh().unwrap();
    assert!(s.complete_refresh(new, catalog(3)));
    assert!(!s.complete_refresh(old, catalog(2)));
    assert_eq!(s.catalog.generation, 3);
}
#[test]
fn refresh_started_before_edit_cannot_replace_optimistic_catalog() {
    let mut s = State::new(catalog(1));
    let refresh = s.begin_refresh().unwrap();
    let save = s.begin_order(Some(&[5, 4, 3, 2, 1]), None).unwrap();
    assert!(!s.complete_refresh(refresh, catalog(2)));
    assert_eq!(s.catalog.project_ids(), [5, 4, 3, 2, 1]);
    assert!(s.finish_order(save, Ok(catalog(3))));
    assert_eq!(s.catalog.generation, 3);
}
#[test]
fn failed_mutation_preserves_newer_snapshot_and_exposes_failure() {
    let mut s = State::new(catalog(1));
    let save = s.begin_order(Some(&[5, 4, 3, 2, 1]), None).unwrap();
    let mut newer = catalog(2);
    newer.projects[0].name = "Renamed".into();
    assert!(s.accept(newer));
    assert!(s.finish_order(save, Err("save rejected".into())));
    assert_eq!(s.catalog.projects[0].name, "Renamed");
    assert_eq!(s.error, "save rejected");
    assert!(!s.pending());
}
#[test]
fn failed_project_reorder_rolls_back_optimistic_order_and_exposes_error() {
    let mut s = State::new(catalog(1));
    let save = s.begin_order(Some(&[5, 4, 3, 2, 1]), None).unwrap();
    assert_eq!(s.catalog.project_ids(), [5, 4, 3, 2, 1]);
    assert!(s.finish_order(save, Err("offline".into())));
    assert_eq!(s.catalog.project_ids(), [1, 2, 3, 4, 5]);
    assert_eq!(s.error, "offline");
    assert!(!s.pending());
}
#[test]
fn mutations_do_not_overlap_and_refresh_waits_during_order_or_drag() {
    let mut s = State::new(catalog(1));
    s.dragging = true;
    assert!(s.begin_refresh().is_none());
    s.dragging = false;
    let save = s.begin_order(None, Some(&[12, 11])).unwrap();
    assert!(s.begin_order(None, Some(&[11, 12])).is_none());
    assert!(s.begin_refresh().is_none());
    s.finish_order(save, Ok(catalog(2)));
    assert!(s.begin_refresh().is_some());
}
#[test]
fn old_generation_and_foreign_owner_are_rejected() {
    let mut s = State::new(catalog(2));
    assert!(!s.accept(catalog(1)));
    let mut other = catalog(9);
    other.owner = 8;
    assert!(!s.accept(other));
    assert_eq!(s.catalog.owner, 7);
}
#[test]
fn disconnected_pending_command_cannot_publish_its_snapshot() {
    let mut s = State::new(catalog(1));
    let save = s.begin_order(None, Some(&[12, 11])).unwrap();
    s.disconnect();
    assert!(!s.finish_order(save, Ok(catalog(9))));
    assert!(!s.accept(catalog(10)));
    assert!(!s.pending());
}
#[test]
fn owner_reset_drops_private_drafts_disclosures_and_old_callbacks() {
    let mut s = State::new(catalog(1));
    s.reveal(Some("P2"));
    s.begin_edit(EditTarget::New { project: Some(2) }, "old trigger".into());
    s.draft("Private draft".into());
    let save = s.begin_order(None, Some(&[12, 11])).unwrap();
    let mut next = catalog(1);
    next.owner = 8;
    s.reset_owner(next);
    assert!(s.edit.is_none());
    assert!(s.expanded.is_empty());
    assert!(s.collapsed_groups.is_empty());
    assert!(!s.finish_order(save, Ok(catalog(11))));
    assert_eq!(s.catalog.owner, 8);
}
#[test]
fn entering_project_reveals_once_and_respects_later_disclosure() {
    let mut s = State::new(catalog(1));
    s.collapsed_groups.insert(11);
    assert_eq!(s.reveal(Some("p2")), Some(2));
    assert!(!s.collapsed_groups.contains(&11));
    assert!(s.expanded.contains(&2));
    s.toggle_group(11);
    s.toggle_project(2);
    assert_eq!(s.reveal(Some("P2")), None);
    s.accept(catalog(2));
    assert_eq!(s.reveal(Some("P2")), None);
    assert!(s.collapsed_groups.contains(&11));
    assert!(!s.expanded.contains(&2));
    assert_eq!(s.reveal(Some("P3")), Some(3));
    assert_eq!(s.reveal(Some("P2")), Some(2));
}
#[test]
fn missing_groups_defer_reveal_until_snapshot_recovers() {
    let mut s = State::new(catalog(1));
    s.groups_ready = false;
    assert_eq!(s.reveal(Some("P2")), None);
    s.accept(catalog(2));
    assert_eq!(s.reveal(Some("P2")), Some(2));
    assert_eq!(s.reveal(Some("P2")), None);
    assert_eq!(s.reveal(None), None);
    assert_eq!(s.reveal(Some("P2")), Some(2));
}
#[test]
fn switching_project_never_toggles_existing_disclosure() {
    let mut s = State::new(catalog(1));
    s.reveal(Some("P1"));
    s.reveal(Some("P2"));
    assert!(s.expanded.contains(&1));
    assert!(s.expanded.contains(&2));
    s.toggle_project(1);
    assert!(s.expanded.contains(&2));
}
#[test]
fn untouched_rename_accepts_updates_while_dirty_draft_stays_local() {
    let mut s = State::new(catalog(1));
    s.begin_edit(EditTarget::Existing(11), "trigger".into());
    let mut update = catalog(2);
    update.groups[0].name = "Remote".into();
    s.accept(update);
    assert_eq!(s.edit.as_ref().unwrap().draft, "Remote");
    s.draft("Local".into());
    let mut newer = catalog(3);
    newer.groups[0].name = "Remote again".into();
    s.accept(newer);
    assert_eq!(s.edit.as_ref().unwrap().draft, "Local");
}
#[test]
fn failed_create_and_rename_preserve_draft_and_focus_anchor() {
    for target in [
        EditTarget::New { project: Some(2) },
        EditTarget::Existing(11),
    ] {
        let mut s = State::new(catalog(1));
        s.begin_edit(target, "trigger".into());
        s.draft(" Draft ".into());
        assert_eq!(s.begin_save().unwrap().1, "Draft");
        assert!(s.cancel_edit().is_none());
        assert!(s.begin_save().is_none());
        assert_eq!(s.finish_save(Err("Conflict".into())), None);
        let edit = s.edit.as_ref().unwrap();
        assert_eq!(edit.draft, " Draft ");
        assert_eq!(edit.return_focus, "trigger");
        assert_eq!(edit.error, "Conflict");
        assert!(!edit.saving);
        assert_eq!(s.cancel_edit(), Some("trigger".into()));
    }
}
#[test]
fn successful_create_clears_draft_and_keeps_pending_assignment_frozen() {
    let mut s = State::new(catalog(1));
    s.begin_edit(EditTarget::New { project: Some(2) }, "trigger".into());
    s.draft("Name".into());
    assert_eq!(
        s.begin_save(),
        Some((EditTarget::New { project: Some(2) }, "Name".into()))
    );
    assert_eq!(s.finish_save(Ok(())), Some("trigger".into()));
    assert!(s.edit.is_none());
}
#[test]
fn blank_group_name_uses_ecmascript_whitespace_and_keeps_edit() {
    let mut s = State::new(catalog(1));
    s.begin_edit(EditTarget::New { project: None }, String::new());
    s.draft("\u{feff}\u{a0}".into());
    assert!(s.begin_save().is_none());
    assert_eq!(s.edit.as_ref().unwrap().error, "Enter a group name.");
    s.draft("\u{0085}".into());
    assert_eq!(s.begin_save().unwrap().1, "\u{0085}");
}
#[test]
fn invalid_order_never_drops_visible_rows() {
    let mut s = State::new(catalog(1));
    for bad in [vec![1, 2], vec![1, 2, 3, 4, 4], vec![1, 2, 3, 4, 99]] {
        assert!(s.begin_order(Some(&bad), None).is_none());
        assert_eq!(s.catalog.project_ids(), [1, 2, 3, 4, 5]);
    }
}
#[test]
fn width_default_scales_without_writing_manual_preference() {
    assert_eq!(
        sizing(None, 16.0),
        Sizing {
            min: 180.0,
            max: 400.0,
            width: 230.0
        }
    );
    assert_eq!(sizing(None, 18.0).width, 258.75);
    assert_eq!(sizing(Some(300.0), 18.0).width, 300.0);
    assert_eq!(sizing(Some(190.0), 18.0).width, 202.5);
    assert_eq!(sizing(Some(190.0), 16.0).width, 190.0);
    assert_eq!(sizing(None, f64::NAN), sizing(None, 16.0));
    assert_eq!(sizing(Some(f64::NAN), 16.0), sizing(None, 16.0));
    assert_eq!(
        sizing(None, 48.0),
        Sizing {
            min: 540.0,
            max: 540.0,
            width: 540.0
        }
    );
}
#[test]
fn one_destination_definition_handles_details_and_overview_settings_alias() {
    assert_eq!(
        DESTINATIONS.map(|d| d.row().0),
        [
            "overview", "issues", "board", "graph", "modules", "pages", "files", "plans",
            "activity", "insights"
        ]
    );
    assert!(Destination::Issues.active("P1", "/p1/issues/P1-22"));
    assert!(!Destination::Issues.active("P1", "/P1/issues_extra"));
    assert!(Destination::Overview.active("P1", "/p1/settings"));
    assert!(Destination::Board.active("P1", "/P1/board"));
    assert!(!Destination::Issues.active("P1", "/P1/board"));
}

#[test]
fn failed_groups_preserve_successful_groups_and_defer_new_project_reveal() {
    let mut state = State::new(catalog(1));
    state.reveal(Some("P1"));
    state.toggle_group(11);
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("Dirty draft".into());
    let mut partial = catalog(2);
    partial.groups.clear();
    assert!(state.accept_partial(partial, false));
    assert_eq!(state.catalog.group_ids(), [11, 12]);
    assert!(!state.groups_ready);
    assert_eq!(state.reveal(Some("P2")), None);
    assert_eq!(state.edit.as_ref().unwrap().draft, "Dirty draft");
    assert!(state.collapsed_groups.contains(&11));
    assert!(state.accept_partial(catalog(3), true));
    assert_eq!(state.reveal(Some("P2")), Some(2));
    assert_eq!(state.reveal(Some("P2")), None);
}
#[test]
fn initial_group_failure_does_not_consume_reveal_or_persisted_disclosure() {
    let mut initial = catalog(1);
    initial.groups.clear();
    let mut state = State::new(initial);
    state.groups_ready = false;
    state.collapsed_groups.extend([11, 12]);
    assert_eq!(state.reveal(Some("P2")), None);
    assert!(!state.expanded.contains(&2));
    assert_eq!(
        state.collapsed_groups.iter().copied().collect::<Vec<_>>(),
        [11, 12]
    );
    assert!(state.accept_partial(catalog(2), true));
    assert_eq!(state.reveal(Some("P2")), Some(2));
    assert!(!state.collapsed_groups.contains(&11));
    assert!(state.collapsed_groups.contains(&12));
}

#[test]
fn old_group_save_success_cannot_close_new_account_editor() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::New { project: Some(2) }, "old trigger".into());
    state.draft("Old private draft".into());
    state.begin_save().unwrap();
    let old = state.save_token().unwrap();
    let mut next = catalog(1);
    next.owner = 8;
    state.reset_owner(next);
    state.begin_edit(EditTarget::Existing(12), "new trigger".into());
    state.draft("New draft".into());
    state.begin_save().unwrap();
    assert_eq!(state.finish_save_token(old, Ok(())), None);
    let edit = state.edit.as_ref().expect("new editor must survive");
    assert_eq!(edit.target, EditTarget::Existing(12));
    assert_eq!(edit.draft, "New draft");
    assert!(edit.saving);
    assert!(edit.error.is_empty());
    assert_eq!(state.catalog.owner, 8);
}
#[test]
fn old_group_save_error_cannot_mutate_same_account_editor_after_epoch_reset() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::New { project: None }, "old trigger".into());
    state.draft("Old draft".into());
    state.begin_save().unwrap();
    let old = state.save_token().unwrap();
    state.reset_owner(catalog(2));
    state.begin_edit(EditTarget::Existing(11), "new trigger".into());
    state.draft("New draft".into());
    state.begin_save().unwrap();
    assert_eq!(
        state.finish_save_token(old, Err("Old save failed".into())),
        None
    );
    let edit = state.edit.as_ref().unwrap();
    assert!(edit.saving);
    assert!(edit.error.is_empty());
    assert_eq!(edit.return_focus, "new trigger");
    assert_eq!(edit.draft, "New draft");
}
#[test]
fn old_group_completion_cannot_close_a_later_same_owner_retry() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("First attempt".into());
    state.begin_save().unwrap();
    let old = state.save_token().unwrap();
    state.finish_save_token(old, Err("Rejected".into()));
    state.draft("Second attempt".into());
    state.begin_save().unwrap();
    let current = state.save_token().unwrap();
    assert_eq!(state.finish_save_token(old, Ok(())), None);
    assert_eq!(state.edit.as_ref().unwrap().draft, "Second attempt");
    assert!(state.edit.as_ref().unwrap().saving);
    assert_eq!(
        state.finish_save_token(current, Ok(())),
        Some("trigger".into())
    );
    assert!(state.edit.is_none());
}
#[test]
fn group_save_blocks_order_and_refresh_admission_until_completion() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("Saving".into());
    state.begin_save().unwrap();
    let token = state.save_token().unwrap();
    assert!(state.pending());
    assert!(state.begin_order(None, Some(&[12, 11])).is_none());
    assert!(state.begin_refresh().is_none());
    assert_eq!(state.catalog.group_ids(), [11, 12]);
    state.finish_save_token(token, Err("Retry".into()));
    assert!(!state.pending());
    assert!(state.begin_order(None, Some(&[12, 11])).is_some());
}
#[test]
fn pending_order_blocks_group_save_without_losing_draft() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("Keep draft".into());
    let token = state.begin_order(None, Some(&[12, 11])).unwrap();
    assert!(state.begin_save().is_none());
    let edit = state.edit.as_ref().unwrap();
    assert!(!edit.saving);
    assert_eq!(edit.draft, "Keep draft");
    state.finish_order(token, Ok(catalog(2)));
    assert!(state.begin_save().is_some());
}
#[test]
fn disconnected_group_save_cannot_complete_or_admit_a_new_save() {
    let mut state = State::new(catalog(1));
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("Keep draft".into());
    state.begin_save().unwrap();
    let old = state.save_token().unwrap();
    state.disconnect();
    assert_eq!(state.finish_save_token(old, Ok(())), None);
    assert!(state.edit.as_ref().is_some());
    assert!(state.begin_save().is_none());
}

#[test]
fn refresh_started_before_group_save_cannot_overwrite_its_editor_or_catalog() {
    let mut state = State::new(catalog(1));
    let refresh = state.begin_refresh().unwrap();
    state.begin_edit(EditTarget::Existing(11), "trigger".into());
    state.draft("Dirty saved draft".into());
    state.begin_save().unwrap();
    let mut stale = catalog(2);
    stale.groups[0].name = "Stale refresh name".into();
    assert!(!state.complete_refresh(refresh, stale));
    assert_eq!(state.catalog.generation, 1);
    assert_eq!(state.catalog.groups[0].name, "Work");
    let edit = state.edit.as_ref().unwrap();
    assert_eq!(edit.draft, "Dirty saved draft");
    assert!(edit.saving);
    assert!(edit.error.is_empty());
}
