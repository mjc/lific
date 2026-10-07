//! Authorized plan reads shared by REST and native views.

use crate::{
    authz,
    db::{
        DbPool,
        models::{CreatePlan, CreatePlanStep, ListPlansQuery, Plan, Role, UpdatePlan},
    },
    error::LificError,
    realtime::{RealtimeEvent, RealtimeHub},
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug, serde::Deserialize)]
pub(crate) struct AddStep {
    pub(crate) parent_step_id: Option<i64>,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) issue_id: Option<i64>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct UpdateStep {
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) done: Option<bool>,
    #[serde(default, deserialize_with = "crate::db::models::deserialize_nullable")]
    pub(crate) issue_id: Option<Option<i64>>,
    pub(crate) move_parent_step_id: Option<i64>,
    pub(crate) move_to_root: Option<bool>,
    pub(crate) move_position: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct StepUpdate {
    pub(crate) plan: Plan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) effect: Option<crate::db::queries::plans::StepDoneEffect>,
}

/// List plans in one project after rechecking the caller's current membership.
pub(crate) fn list_for_project(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<Vec<Plan>, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let current = crate::auth::refresh_identity(&tx, identity.as_ref())?;
    authz::require_role_conn(&tx, &current, project_id, Role::Viewer)?;
    let mut plans = Vec::new();
    let mut before_id = None;
    loop {
        let page = crate::db::queries::plans::list_plans(
            &tx,
            &ListPlansQuery {
                project_id: Some(project_id),
                limit: Some(500),
                order_by: Some("id".into()),
                before_id,
                ..Default::default()
            },
        )?;
        if page.len() < 500 {
            plans.extend(page);
            break;
        }
        before_id = page.last().map(|plan| plan.id);
        plans.extend(page);
    }
    plans.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    tx.commit()?;
    Ok(plans)
}

/// Read one plan only after authorizing its owning project.
pub(crate) fn get(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    plan_id: i64,
) -> Result<Plan, LificError> {
    let conn = db.read()?;
    let plan = crate::db::queries::plans::get_plan(&conn, plan_id)?;
    authz::require_role(db, identity, plan.project_id, Role::Viewer)?;
    Ok(plan)
}

fn require_issue_project_role(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    issue_id: i64,
) -> Result<(), LificError> {
    let issue = {
        let conn = db.read()?;
        crate::db::queries::get_issue(&conn, issue_id)?
    };
    authz::require_role(db, identity, issue.project_id, Role::Maintainer)
}

fn require_create_issue_roles(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    input: &CreatePlan,
) -> Result<(), LificError> {
    fn collect(steps: &[CreatePlanStep], ids: &mut Vec<i64>) {
        for step in steps {
            ids.extend(step.issue_id);
            collect(&step.steps, ids);
        }
    }
    let mut ids: Vec<_> = input.issue_id.into_iter().collect();
    collect(&input.steps, &mut ids);
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .try_for_each(|issue_id| require_issue_project_role(db, identity, issue_id))
}

/// Create using REST's Maintainer and cross-project linked-issue gates.
pub(crate) fn create(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    input: &CreatePlan,
) -> Result<Plan, LificError> {
    authz::require_role(db, identity, input.project_id, Role::Maintainer)?;
    require_create_issue_roles(db, identity, input)?;
    let plan = {
        let conn = db.write()?;
        crate::db::queries::plans::create_plan(&conn, input)?
    };
    realtime.send(RealtimeEvent::ProjectUpdated {
        project_id: plan.project_id,
    });
    Ok(plan)
}

pub(crate) fn update(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    input: &UpdatePlan,
) -> Result<Plan, LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::plans::get_plan(&conn, id)?.project_id
    };
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    if let Some(Some(issue_id)) = input.issue_id {
        require_issue_project_role(db, identity, issue_id)?;
    }
    let plan = {
        let conn = db.write()?;
        crate::db::queries::plans::update_plan(&conn, id, input)?
    };
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(plan)
}

pub(crate) fn delete(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<(), LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::plans::get_plan(&conn, id)?.project_id
    };
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    {
        let conn = db.write()?;
        crate::db::queries::plans::delete_plan(&conn, id)?;
    }
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(())
}

pub(crate) fn add_step(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    plan_id: i64,
    input: &AddStep,
) -> Result<Plan, LificError> {
    let project_id = {
        let conn = db.read()?;
        crate::db::queries::plans::get_plan(&conn, plan_id)?.project_id
    };
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    if let Some(issue_id) = input.issue_id {
        require_issue_project_role(db, identity, issue_id)?;
    }
    let plan = {
        let conn = db.write()?;
        crate::db::queries::plans::add_step(
            &conn,
            plan_id,
            input.parent_step_id,
            &input.title,
            &input.description,
            input.issue_id,
        )?;
        crate::db::queries::plans::get_plan(&conn, plan_id)?
    };
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(plan)
}

pub(crate) fn update_step(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    plan_id: i64,
    step_id: i64,
    input: &UpdateStep,
) -> Result<StepUpdate, LificError> {
    use crate::db::queries::plans;
    let project_id = {
        let conn = db.read()?;
        plans::get_plan(&conn, plan_id)?.project_id
    };
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    if let Some(Some(issue_id)) = input.issue_id {
        require_issue_project_role(db, identity, issue_id)?;
    }
    if input.done == Some(true) {
        let linked = {
            let conn = db.read()?;
            plans::assert_step_in_plan(&conn, plan_id, step_id)?;
            plans::step_issue_id(&conn, step_id)?
        };
        if let Some(issue_id) = linked {
            require_issue_project_role(db, identity, issue_id)?;
        }
    }
    let (response, issue_event) = {
        let conn = db.write()?;
        plans::assert_step_in_plan(&conn, plan_id, step_id)?;
        if let Some(title) = &input.title {
            plans::set_step_title(&conn, step_id, title)?;
        }
        if let Some(description) = &input.description {
            plans::set_step_description(&conn, step_id, description)?;
        }
        if let Some(issue_id) = input.issue_id {
            plans::set_step_issue(&conn, step_id, issue_id)?;
        }
        let effect = input
            .done
            .map(|done| plans::set_step_done(&conn, step_id, done))
            .transpose()?;
        if input.move_to_root.unwrap_or(false)
            || input.move_parent_step_id.is_some()
            || input.move_position.is_some()
        {
            let parent = if input.move_to_root.unwrap_or(false) {
                None
            } else if let Some(parent) = input.move_parent_step_id {
                Some(parent)
            } else {
                plans::step_parent(&conn, step_id)?
            };
            plans::move_step(&conn, step_id, parent, input.move_position)?;
        }
        let plan = plans::get_plan(&conn, plan_id)?;
        let issue_event = if effect
            .as_ref()
            .is_some_and(|effect| effect.issue_status_changed)
        {
            plans::step_issue_id(&conn, step_id)?
                .map(|issue_id| {
                    crate::db::queries::get_issue(&conn, issue_id)
                        .map(|issue| (issue.project_id, issue.id))
                })
                .transpose()?
        } else {
            None
        };
        (StepUpdate { plan, effect }, issue_event)
    };
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    if let Some((issue_project_id, issue_id)) = issue_event {
        realtime.send(RealtimeEvent::IssueUpdated {
            project_id: issue_project_id,
            issue_id,
        });
    }
    Ok(response)
}

pub(crate) fn delete_step(
    db: &DbPool,
    realtime: &RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    plan_id: i64,
    step_id: i64,
) -> Result<Plan, LificError> {
    use crate::db::queries::plans;
    let project_id = {
        let conn = db.read()?;
        plans::get_plan(&conn, plan_id)?.project_id
    };
    authz::require_role(db, identity, project_id, Role::Maintainer)?;
    let plan = {
        let conn = db.write()?;
        plans::assert_step_in_plan(&conn, plan_id, step_id)?;
        plans::delete_step(&conn, step_id)?;
        plans::get_plan(&conn, plan_id)?
    };
    realtime.send(RealtimeEvent::ProjectUpdated { project_id });
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actor::Transport,
        db::{models::CreatePlan, queries},
    };

    #[test]
    fn project_list_keeps_master_updated_order_and_requires_current_membership() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        {
            let conn = db.write().unwrap();
            for title in ["Older plan", "Newer plan"] {
                queries::plans::create_plan(
                    &conn,
                    &CreatePlan {
                        project_id,
                        title: title.into(),
                        issue_id: None,
                        steps: Vec::new(),
                    },
                )
                .unwrap();
            }
            conn.execute(
                "UPDATE plans SET updated_at=CASE title WHEN 'Older plan' THEN '2025-01-01' ELSE '2025-02-01' END WHERE project_id=?1",
                [project_id],
            )
            .unwrap();
        }

        let listed = list_for_project(&db, &identity, project_id).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|plan| plan.title.as_str())
                .collect::<Vec<_>>(),
            ["Newer plan", "Older plan"]
        );

        db.write()
            .unwrap()
            .execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![project_id, viewer.id],
            )
            .unwrap();
        assert!(matches!(
            list_for_project(&db, &identity, project_id),
            Err(LificError::Forbidden(_))
        ));
    }

    #[test]
    fn project_list_reads_past_master_page_size() {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
        {
            let conn = db.write().unwrap();
            for index in 0..501 {
                queries::plans::create_plan(
                    &conn,
                    &CreatePlan {
                        project_id,
                        title: format!("Plan {index}"),
                        issue_id: None,
                        steps: Vec::new(),
                    },
                )
                .unwrap();
            }
        }

        assert_eq!(
            list_for_project(&db, &identity, project_id).unwrap().len(),
            501
        );
    }
}
