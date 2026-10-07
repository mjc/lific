use crate::{
    authz,
    db::{
        DbPool,
        models::{CreateModule, Issue, ListIssuesQuery, Module, UpdateModule},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug)]
pub(crate) struct ModuleSummary {
    pub(crate) module: Module,
    pub(crate) issue_count: usize,
    pub(crate) done_count: usize,
}

#[derive(Debug)]
pub(crate) struct ModuleList {
    pub(crate) modules: Vec<ModuleSummary>,
    pub(crate) total_issues: usize,
    pub(crate) done_issues: usize,
    pub(crate) active_modules: usize,
}

#[derive(Debug)]
pub(crate) struct ModuleDetail {
    pub(crate) module: Module,
    pub(crate) issues: Vec<Issue>,
}

pub(crate) fn list(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<ModuleList, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let result = list_conn(&tx, identity, project_id)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn list_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    project_id: i64,
) -> Result<ModuleList, LificError> {
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &identity, project_id, crate::db::models::Role::Viewer)?;
    let modules = queries::list_modules(conn, project_id)?;
    let counts = conn.prepare_cached(
        "SELECT module_id, COUNT(*), SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END)
         FROM issues WHERE project_id = ?1 AND module_id IS NOT NULL AND deleted_at IS NULL GROUP BY module_id",
    )?.query_map([project_id], |row| {
        Ok((row.get::<_, i64>(0)?, (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)))
    })?
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    let modules = modules
        .into_iter()
        .map(|module| {
            let (issue_count, done_count) = counts.get(&module.id).copied().unwrap_or_default();
            let issue_count = usize::try_from(issue_count).unwrap_or(0);
            let done_count = usize::try_from(done_count).unwrap_or(0);
            ModuleSummary {
                module,
                issue_count,
                done_count,
            }
        })
        .collect::<Vec<_>>();
    let total_issues = modules.iter().map(|summary| summary.issue_count).sum();
    let done_issues = modules.iter().map(|summary| summary.done_count).sum();
    let active_modules = modules
        .iter()
        .filter(|summary| summary.module.status == "active")
        .count();
    Ok(ModuleList {
        modules,
        total_issues,
        done_issues,
        active_modules,
    })
}

#[cfg(test)]
pub(crate) fn detail(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    module_id: i64,
) -> Result<ModuleDetail, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let result = detail_conn(&tx, identity, module_id)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn get(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    module_id: i64,
) -> Result<Module, LificError> {
    let conn = db.read()?;
    let tx = conn.unchecked_transaction()?;
    let module = get_conn(&tx, identity, module_id)?;
    tx.commit()?;
    Ok(module)
}

pub(crate) fn get_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    module_id: i64,
) -> Result<Module, LificError> {
    let project_id = queries::get_resource_project_id(
        conn,
        crate::db::queries::ResourceTable::Modules,
        module_id,
    )?;
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &identity, project_id, crate::db::models::Role::Viewer)?;
    queries::get_module(conn, module_id)
}

pub(crate) fn detail_conn(
    conn: &rusqlite::Connection,
    identity: &Option<ResolvedIdentity>,
    module_id: i64,
) -> Result<ModuleDetail, LificError> {
    let project_id = queries::get_resource_project_id(
        conn,
        crate::db::queries::ResourceTable::Modules,
        module_id,
    )?;
    let identity = crate::auth::refresh_identity(conn, identity.as_ref())?;
    authz::require_role_conn(conn, &identity, project_id, crate::db::models::Role::Viewer)?;
    let module = queries::get_module(conn, module_id)?;
    let mut issues = queries::list_issues(
        conn,
        &ListIssuesQuery {
            project_id: Some(project_id),
            module_id: Some(module_id),
            limit: Some(500),
            ..Default::default()
        },
    )?;
    crate::services::issues::retain_visible_relations_conn(conn, &identity, &mut issues)?;
    Ok(ModuleDetail { module, issues })
}

pub(crate) fn create(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    input: CreateModule,
) -> Result<Module, LificError> {
    let module = db.transaction(|tx| {
        let current = crate::auth::refresh_identity(tx, identity.as_ref())?;
        authz::require_structure_role_conn(tx, &current, input.project_id)?;
        queries::create_module(tx, &input)
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectUpdated {
        project_id: module.project_id,
    });
    Ok(module)
}

pub(crate) fn update(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    input: UpdateModule,
) -> Result<Module, LificError> {
    update_scoped(db, realtime, identity, id, None, input)
}

pub(crate) fn update_scoped(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    expected_project_id: Option<i64>,
    input: UpdateModule,
) -> Result<Module, LificError> {
    let module = db.transaction(|tx| {
        let project_id =
            queries::get_resource_project_id(tx, crate::db::queries::ResourceTable::Modules, id)?;
        if expected_project_id.is_some_and(|expected| expected != project_id) {
            return Err(LificError::NotFound(format!("module {id} not found")));
        }
        let current = crate::auth::refresh_identity(tx, identity.as_ref())?;
        authz::require_structure_role_conn(tx, &current, project_id)?;
        queries::update_module(tx, id, &input)
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectUpdated {
        project_id: module.project_id,
    });
    Ok(module)
}

pub(crate) fn delete(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
) -> Result<(), LificError> {
    delete_scoped(db, realtime, identity, id, None)
}

pub(crate) fn delete_scoped(
    db: &DbPool,
    realtime: &crate::realtime::RealtimeHub,
    identity: &Option<ResolvedIdentity>,
    id: i64,
    expected_project_id: Option<i64>,
) -> Result<(), LificError> {
    let project_id = db.transaction(|tx| {
        let project_id =
            queries::get_resource_project_id(tx, crate::db::queries::ResourceTable::Modules, id)?;
        if expected_project_id.is_some_and(|expected| expected != project_id) {
            return Err(LificError::NotFound(format!("module {id} not found")));
        }
        let current = crate::auth::refresh_identity(tx, identity.as_ref())?;
        authz::require_structure_role_conn(tx, &current, project_id)?;
        queries::delete_module(tx, id)?;
        Ok(project_id)
    })?;
    realtime.send(crate::realtime::RealtimeEvent::ProjectUpdated { project_id });
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{actor::Transport, auth::fresh_identity, db::queries, services::modules};

    fn fixture() -> (
        crate::db::DbPool,
        crate::db::models::User,
        Option<crate::resolve_caller::ResolvedIdentity>,
        i64,
    ) {
        let (db, _, _, _, viewer, _, project_id) =
            crate::api::test_helpers::setup_membership_test();
        let identity = Some(fresh_identity(&viewer, Transport::Web));
        (db, viewer, identity, project_id)
    }

    #[test]
    fn module_list_returns_authoritative_issue_counts_and_progress_for_every_module() {
        let (db, _, identity, project_id) = fixture();
        let module = {
            let conn = db.write().unwrap();
            queries::create_module(
                &conn,
                &crate::db::models::CreateModule {
                    project_id,
                    name: "Foundation".into(),
                    description: String::new(),
                    status: "active".into(),
                    emoji: None,
                },
            )
            .unwrap()
        };
        {
            let conn = db.write().unwrap();
            for index in 0..501 {
                let issue = queries::create_issue(
                    &conn,
                    &crate::db::models::CreateIssue {
                        project_id,
                        title: format!("Module task {index}"),
                        status: if index == 0 {
                            crate::db::models::Status::Done
                        } else {
                            crate::db::models::Status::Todo
                        },
                        module_id: Some(module.id),
                        ..Default::default()
                    },
                )
                .unwrap();
                if index == 0 {
                    assert_eq!(issue.module_id, Some(module.id));
                }
            }
        }
        let result = modules::list(&db, &identity, project_id).unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].module.status, "active");
        assert_eq!(result.modules[0].issue_count, 501);
        assert_eq!(result.modules[0].done_count, 1);
    }

    #[test]
    fn module_detail_returns_assigned_issues_for_a_viewer() {
        let (db, _, identity, project_id) = fixture();
        let module = {
            let conn = db.write().unwrap();
            queries::create_module(
                &conn,
                &crate::db::models::CreateModule {
                    project_id,
                    name: "Private module".into(),
                    description: String::new(),
                    status: "planned".into(),
                    emoji: None,
                },
            )
            .unwrap()
        };
        let detail = modules::detail(&db, &identity, module.id).unwrap();
        assert_eq!(detail.module.id, module.id);
    }

    #[test]
    fn module_progress_ignores_tombstoned_issues() {
        let (db, _, identity, project_id) = fixture();
        let module = {
            let conn = db.write().unwrap();
            queries::create_module(
                &conn,
                &crate::db::models::CreateModule {
                    project_id,
                    name: "Deleted work".into(),
                    description: String::new(),
                    status: "active".into(),
                    emoji: None,
                },
            )
            .unwrap()
        };
        {
            let conn = db.write().unwrap();
            let issue = queries::create_issue(
                &conn,
                &crate::db::models::CreateIssue {
                    project_id,
                    title: "Deleted task".into(),
                    module_id: Some(module.id),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE issues SET deleted_at='2026-10-06T00:00:00Z' WHERE id=?1",
                [issue.id],
            )
            .unwrap();
        }
        let result = modules::list(&db, &identity, project_id).unwrap();
        assert_eq!(result.modules[0].issue_count, 0);
        assert_eq!(result.total_issues, 0);
    }
}
