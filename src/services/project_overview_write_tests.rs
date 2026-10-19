use super::*;
use crate::{
    actor::Transport,
    db::models::{CreateIssue, CreateLabel, CreatePage, UpdateProject},
};
#[test]
fn overview_label_mutations_keep_legacy_lead_and_enforced_maintainer_gates() {
    for enforced in [false, true] {
        let (db, _, lead, maintainer, viewer, _, project) =
            crate::api::test_helpers::setup_membership_test();
        queries::settings::update(
            &db.write().unwrap(),
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(enforced),
                ..Default::default()
            },
        )
        .unwrap();
        let hub = crate::realtime::RealtimeHub::new();
        let mut events = hub.subscribe();
        for user in [&viewer, &maintainer, &lead] {
            let result = label(
                &db,
                &hub,
                &Some(crate::auth::fresh_identity(user, Transport::Web)),
                project,
                LabelCommand::Create {
                    name: user.username.clone(),
                    color: "#EF4444".into(),
                },
            );
            if user.id == lead.id || (enforced && user.id == maintainer.id) {
                assert!(result.is_ok());
                events.try_recv().unwrap();
            } else {
                assert!(matches!(result, Err(LificError::Forbidden(_))));
            }
            assert!(events.try_recv().is_err());
        }
        assert_eq!(
            queries::list_labels(&db.read().unwrap(), project)
                .unwrap()
                .len(),
            if enforced { 2 } else { 1 }
        );
    }
}
#[test]
fn overview_label_merge_reassigns_issue_and_page_usage_once_and_refuses_other_projects() {
    let (db, _, _, maintainer, _, _, project) = crate::api::test_helpers::setup_membership_test();
    let (source, target, foreign, issue, page) = {
        let conn = db.write().unwrap();
        let mk = |project_id, name: &str| {
            queries::create_label(
                &conn,
                &CreateLabel {
                    project_id,
                    name: name.into(),
                    color: "#EF4444".into(),
                },
            )
            .unwrap()
        };
        let source = mk(project, "source");
        let target = mk(project, "target");
        let other = queries::create_project(
            &conn,
            &crate::db::models::CreateProject {
                identifier: "OTHER".into(),
                name: "Other".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let foreign = mk(other.id, "foreign");
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project,
                title: "Labels".into(),
                labels: vec!["source".into(), "target".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project),
                title: "Labels".into(),
                labels: vec!["source".into(), "target".into()],
                ..Default::default()
            },
        )
        .unwrap();
        (source.id, target.id, foreign.id, issue.id, page.id)
    };
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(crate::auth::fresh_identity(&maintainer, Transport::Web));
    assert!(matches!(
        label(
            &db,
            &hub,
            &caller,
            project,
            LabelCommand::Merge {
                id: source,
                into: foreign
            }
        ),
        Err(LificError::BadRequest(_))
    ));
    assert!(events.try_recv().is_err());
    assert_eq!(
        queries::list_labels(&db.read().unwrap(), project)
            .unwrap()
            .len(),
        2
    );
    label(
        &db,
        &hub,
        &caller,
        project,
        LabelCommand::Merge {
            id: source,
            into: target,
        },
    )
    .unwrap();
    events.try_recv().unwrap();
    assert!(events.try_recv().is_err());
    assert_eq!(
        queries::get_issue(&db.read().unwrap(), issue)
            .unwrap()
            .labels,
        ["target"]
    );
    assert_eq!(
        queries::get_page(&db.read().unwrap(), page).unwrap().labels,
        ["target"]
    );
    assert_eq!(
        queries::list_labels(&db.read().unwrap(), project)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn overview_project_name_publish_and_lead_commands_write_exact_fields_and_one_event() {
    let (db, _, lead, _, _, other, project) = crate::api::test_helpers::setup_membership_test();
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let caller = Some(crate::auth::fresh_identity(&lead, Transport::Web));
    let initial = queries::get_project(&db.read().unwrap(), project).unwrap();
    let public = update(
        &db,
        &hub,
        &caller,
        None,
        project,
        UpdateProject {
            is_public: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(public.is_public);
    assert_eq!(public.name, initial.name);
    assert_eq!(public.lead_user_id, initial.lead_user_id);
    assert_eq!(
        events.try_recv().unwrap().event,
        crate::realtime::RealtimeEvent::ProjectUpdated {
            project_id: project
        }
    );
    assert!(events.try_recv().is_err());
    assert!(
        matches!(update(&db,&hub,&caller,None,project,UpdateProject{lead_user_id:Some(Some(other.id)),..Default::default()}),Err(LificError::Forbidden(message)) if message=="recent authentication required")
    );
    assert!(events.try_recv().is_err());
    let token = queries::users::create_session(&db.write().unwrap(), lead.id, None)
        .unwrap()
        .token;
    let project_row = update(
        &db,
        &hub,
        &caller,
        Some(&token),
        project,
        UpdateProject {
            lead_user_id: Some(Some(other.id)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(project_row.lead_user_id, Some(other.id));
    assert_eq!(
        queries::members::get_member_role(&db.read().unwrap(), project, other.id).unwrap(),
        Some(Role::Lead)
    );
    events.try_recv().unwrap();
    assert!(events.try_recv().is_err());
}
#[test]
fn overview_update_label_and_delete_refuse_stale_admin_authority_inside_writer() {
    let (db, admin, _, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
    let caller = Some(crate::auth::fresh_identity(&admin, Transport::Web));
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    db.write()
        .unwrap()
        .execute("UPDATE users SET is_admin=0 WHERE id=?1", [admin.id])
        .unwrap();
    assert!(matches!(
        update(
            &db,
            &hub,
            &caller,
            None,
            project,
            UpdateProject {
                name: Some("Forbidden".into()),
                ..Default::default()
            }
        ),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        label(
            &db,
            &hub,
            &caller,
            project,
            LabelCommand::Create {
                name: "Forbidden".into(),
                color: "#EF4444".into()
            }
        ),
        Err(LificError::Forbidden(_))
    ));
    assert!(matches!(
        delete(&db, &hub, &caller, project),
        Err(LificError::Forbidden(_))
    ));
    assert!(events.try_recv().is_err());
    assert!(queries::get_project(&db.read().unwrap(), project).is_ok());
    assert!(
        queries::list_labels(&db.read().unwrap(), project)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn overview_delete_keeps_legacy_admin_gate_and_enforced_lead_gate() {
    for enforced in [false, true] {
        let (db, admin, lead, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
        queries::settings::update(
            &db.write().unwrap(),
            queries::settings::InstanceSettingsPatch {
                authz_enforced: Some(enforced),
                ..Default::default()
            },
        )
        .unwrap();
        let hub = crate::realtime::RealtimeHub::new();
        let mut events = hub.subscribe();
        if enforced {
            delete(
                &db,
                &hub,
                &Some(crate::auth::fresh_identity(&lead, Transport::Web)),
                project,
            )
            .unwrap();
        } else {
            assert!(matches!(
                delete(
                    &db,
                    &hub,
                    &Some(crate::auth::fresh_identity(&lead, Transport::Web)),
                    project
                ),
                Err(LificError::Forbidden(_))
            ));
            assert!(events.try_recv().is_err());
            delete(
                &db,
                &hub,
                &Some(crate::auth::fresh_identity(&admin, Transport::Web)),
                project,
            )
            .unwrap();
        }
        assert_eq!(
            events.try_recv().unwrap().event,
            crate::realtime::RealtimeEvent::ProjectDeleted {
                project_id: project
            }
        );
        assert!(events.try_recv().is_err());
        assert!(matches!(
            queries::get_project(&db.read().unwrap(), project),
            Err(LificError::NotFound(_))
        ));
    }
}

#[test]
fn overview_confirmed_delete_refuses_identifier_changed_since_confirmation() {
    let (db, _, lead, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
    let caller = Some(crate::auth::fresh_identity(&lead, Transport::Web));
    let hub = crate::realtime::RealtimeHub::new();
    let mut events = hub.subscribe();
    let expected = queries::get_project(&db.read().unwrap(), project)
        .unwrap()
        .identifier;
    let issue = queries::create_issue(
        &db.write().unwrap(),
        &CreateIssue {
            project_id: project,
            title: "Keep this content when stale deletion is refused".into(),
            ..Default::default()
        },
    )
    .unwrap();

    // Another caller rekeys after the user captured the displayed identifier.
    update(
        &db,
        &hub,
        &caller,
        None,
        project,
        UpdateProject {
            identifier: Some("REN".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        events.try_recv().unwrap().event,
        crate::realtime::RealtimeEvent::ProjectUpdated {
            project_id: project
        }
    );
    assert!(events.try_recv().is_err());

    let result = delete_confirmed(&db, &hub, &caller, project, &expected);
    assert!(matches!(
        result,
        Err(LificError::BadRequest(ref message))
            if message == "Type the project's exact identifier to confirm."
    ));
    assert!(events.try_recv().is_err());
    assert_eq!(
        queries::get_project(&db.read().unwrap(), project)
            .unwrap()
            .identifier,
        "REN"
    );
    assert!(queries::get_issue(&db.read().unwrap(), issue.id).is_ok());

    delete_confirmed(&db, &hub, &caller, project, "REN").unwrap();
    assert_eq!(
        events.try_recv().unwrap().event,
        crate::realtime::RealtimeEvent::ProjectDeleted {
            project_id: project
        }
    );
    assert!(events.try_recv().is_err());
    assert!(matches!(
        queries::get_project(&db.read().unwrap(), project),
        Err(LificError::NotFound(_))
    ));
    assert!(matches!(
        queries::get_issue(&db.read().unwrap(), issue.id),
        Err(LificError::NotFound(_))
    ));
}
