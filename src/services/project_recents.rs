//! Five private sidebar rows; the original bounded reads and ordering live here.
//! Callers resolve their current browser cookie before entering this service.
use crate::{
    actor::Transport,
    db::{
        DbPool,
        models::{Issue, ListIssuesQuery, ListPlansQuery, Module, Page, Plan, Role},
        queries,
    },
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Section {
    Issues,
    Modules,
    Pages,
    Plans,
}
impl Section {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Issues => "issues",
            Self::Modules => "modules",
            Self::Pages => "pages",
            Self::Plans => "plans",
        }
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Issues => 0,
            Self::Modules => 1,
            Self::Pages => 2,
            Self::Plans => 3,
        }
    }
    pub(crate) fn for_path(identifier: &str, path: &str) -> Option<Self> {
        let section = path
            .strip_prefix('/')?
            .strip_prefix(identifier)?
            .strip_prefix('/')?
            .split('/')
            .next()?;
        match section {
            "issues" => Some(Self::Issues),
            "modules" => Some(Self::Modules),
            "pages" => Some(Self::Pages),
            "plans" => Some(Self::Plans),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Row {
    pub(crate) href: String,
    pub(crate) label: String,
    pub(crate) identifier: Option<String>,
}
impl Row {
    pub(crate) fn accessible_name(&self) -> String {
        self.identifier
            .as_ref()
            .map_or_else(|| self.label.clone(), |id| format!("{id}: {}", self.label))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ReadFailure {
    pub(crate) access: bool,
    pub(crate) message: String,
}
impl From<LificError> for ReadFailure {
    fn from(error: LificError) -> Self {
        let access = matches!(&error, LificError::Forbidden(_) | LificError::NotFound(_));
        let message = match error {
            LificError::Forbidden(message) | LificError::NotFound(message) => message,
            error => {
                tracing::warn!(error = %error, "native sidebar recents read failed");
                "Could not load recent resources.".into()
            }
        };
        Self { access, message }
    }
}
fn row(
    project: &str,
    section: Section,
    id: String,
    label: String,
    identifier: Option<String>,
) -> Row {
    Row {
        href: format!("/{project}/{}/{id}", section.as_str()),
        label,
        identifier,
    }
}
pub(crate) fn issue_rows(project: &str, rows: Vec<Issue>) -> Vec<Row> {
    rows.into_iter()
        .take(5)
        .map(|item| {
            row(
                project,
                Section::Issues,
                item.identifier.clone(),
                item.title,
                Some(item.identifier),
            )
        })
        .collect()
}
pub(crate) fn module_rows(project: &str, mut rows: Vec<Module>) -> Vec<Row> {
    // Stable sort retains the server's name order for equal update timestamps.
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    rows.into_iter()
        .take(5)
        .map(|item| {
            row(
                project,
                Section::Modules,
                item.id.to_string(),
                item.name,
                None,
            )
        })
        .collect()
}
pub(crate) fn page_rows(
    project: &str,
    results: [Result<Vec<Page>, ReadFailure>; 3],
) -> Result<Vec<Row>, ReadFailure> {
    // Access denial wins over an earlier transient sibling fault. Publish only
    // complete lifecycle snapshots; never replace a cache with a partial list.
    if let Some(error) = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .find(|error| error.access)
    {
        return Err(error.clone());
    }
    let mut rows = Vec::new();
    for result in results {
        rows.extend(result?);
    }
    rows.retain(|item| ["draft", "active", "complete"].contains(&item.status.as_str()));
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(rows
        .into_iter()
        .take(5)
        .map(|item| {
            row(
                project,
                Section::Pages,
                item.id.to_string(),
                item.title,
                None,
            )
        })
        .collect())
}
pub(crate) fn plan_rows(project: &str, rows: Vec<Plan>) -> Vec<Row> {
    rows.into_iter()
        .filter(|item| item.status != "archived")
        .take(5)
        .map(|item| {
            row(
                project,
                Section::Plans,
                item.id.to_string(),
                item.title,
                None,
            )
        })
        .collect()
}
/// No browser catalog fields are used for read authority or link metadata.
/// Fresh account, role and project are read in the same SQLite read snapshot.
pub(crate) fn load(
    db: &DbPool,
    identity: &Option<ResolvedIdentity>,
    owner: i64,
    project_id: i64,
    section: Section,
) -> Result<Vec<Row>, ReadFailure> {
    let read = || -> Result<Vec<Row>, ReadFailure> {
        let supplied = crate::api::require_user(identity)?;
        if supplied.id != owner {
            return Err(
                LificError::Forbidden("Your account changed. Reload this page.".into()).into(),
            );
        }
        let conn = db.read()?;
        let tx = conn.unchecked_transaction().map_err(LificError::from)?;
        let user = crate::auth::fresh_caller(&tx, owner)?;
        if user.is_bot {
            return Err(
                LificError::Forbidden("A signed-in browser account is required.".into()).into(),
            );
        }
        let fresh = Some(crate::auth::fresh_identity(&user, Transport::Web));
        crate::authz::require_role_conn(&tx, &fresh, project_id, Role::Viewer)?;
        let project = queries::get_project(&tx, project_id)?;
        let rows = match section {
            Section::Issues => issue_rows(
                &project.identifier,
                queries::list_issues(
                    &tx,
                    &ListIssuesQuery {
                        project_id: Some(project_id),
                        order_by: Some("updated".into()),
                        order: Some("desc".into()),
                        limit: Some(5),
                        ..Default::default()
                    },
                )?,
            ),
            Section::Modules => {
                module_rows(&project.identifier, queries::list_modules(&tx, project_id)?)
            }
            Section::Pages => {
                let results = ["draft", "active", "complete"].map(|status| {
                    queries::list_pages(
                        &tx,
                        Some(project_id),
                        None,
                        None,
                        Some(status),
                        Some("updated"),
                        Some("desc"),
                        Some(5),
                        None,
                    )
                    .map_err(ReadFailure::from)
                });
                page_rows(&project.identifier, results)?
            }
            Section::Plans => plan_rows(
                &project.identifier,
                queries::plans::list_plans(
                    &tx,
                    &ListPlansQuery {
                        project_id: Some(project_id),
                        limit: Some(10),
                        ..Default::default()
                    },
                )?,
            ),
        };
        tx.commit().map_err(LificError::from)?;
        Ok(rows)
    };
    read()
}
#[cfg(test)]
#[path = "project_recents_tests.rs"]
mod tests;
