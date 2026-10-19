use crate::{
    db::models::{CreateIssue, Priority, Status},
    error::LificError,
};

pub(crate) struct Defaults {
    pub(crate) module_id: Option<i64>,
    pub(crate) status: Status,
}

pub(crate) fn defaults(module_id: Option<&str>, status: Option<&str>) -> Defaults {
    Defaults {
        module_id: module_id
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|id| *id > 0),
        status: status
            .and_then(|value| value.parse::<Status>().ok())
            .unwrap_or_default(),
    }
}

pub(crate) struct Draft {
    pub(crate) project_id: i64,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: Status,
    pub(crate) priority: Priority,
    pub(crate) module_id: Option<i64>,
    pub(crate) labels: Vec<String>,
}

impl Draft {
    pub(crate) fn input(&self) -> Result<CreateIssue, LificError> {
        let title = super::super::super::runtime::whitespace::trim_ecmascript(&self.title);
        if title.is_empty() {
            return Err(LificError::BadRequest("issue title is required".into()));
        }
        Ok(CreateIssue {
            project_id: self.project_id,
            title: title.to_owned(),
            description: self.description.clone(),
            status: self.status,
            priority: self.priority,
            module_id: self.module_id,
            labels: self.labels.clone(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_statuses_and_priorities_parse_from_form_values() {
        for value in ["backlog", "todo", "active", "done", "cancelled"] {
            assert_eq!(value.parse::<Status>().unwrap().as_str(), value);
        }
        for value in ["urgent", "high", "medium", "low", "none"] {
            assert_eq!(value.parse::<Priority>().unwrap().as_str(), value);
        }
    }
}
