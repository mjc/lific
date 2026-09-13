//! An owned data-command result. Rendering never accesses a backend.

use super::{
    CommentAction, DataCommand, ExportAction, FolderAction, IssueAction, LabelAction, ModuleAction,
    PageAction, ProjectAction, render,
};
use crate::db::models;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::HashMap, io::Write, path::PathBuf};

/// Preserve the server's JSON, including fields unknown to this client. Display
/// metadata belongs to this result, so rendering is repeatable and order independent.
#[derive(Debug)]
pub struct CommandOutput {
    pub value: Value,
    pub module_names: HashMap<i64, String>,
    pub continuation: Option<render::CommentContinuation>,
}

impl CommandOutput {
    pub fn new(value: Value) -> Self {
        Self {
            value,
            module_names: HashMap::new(),
            continuation: None,
        }
    }

    pub fn from_value(value: &impl Serialize) -> Result<Self, serde_json::Error> {
        serde_json::to_value(value).map(Self::new)
    }

    pub fn human(&self, command: &DataCommand) -> String {
        self.render(command)
            .unwrap_or_else(|| format!("{}\n", self.value_pretty()))
    }

    fn value_pretty(&self) -> String {
        // A serde_json::Value has no fallible custom serializer.
        serde_json::to_string_pretty(&self.value).expect("JSON value serialization")
    }

    pub fn write(
        &self,
        command: &DataCommand,
        json: bool,
        writer: &mut impl Write,
    ) -> std::io::Result<()> {
        if json {
            writeln!(writer, "{}", self.value_pretty())
        } else {
            writer.write_all(self.human(command).as_bytes())
        }
    }

    fn render(&self, command: &DataCommand) -> Option<String> {
        let value = &self.value;
        Some(match command {
            DataCommand::Issue { action } => match action {
                IssueAction::List { .. } => {
                    let issues: Vec<models::Issue> = decode(value)?;
                    let names = &self.module_names;
                    render::issue_list(&issues, &|id| names.get(&id).cloned())
                }
                IssueAction::Get { .. } => {
                    let issue: models::Issue = decode(value)?;
                    let names = &self.module_names;
                    render::issue_detail(&issue, &|id| names.get(&id).cloned())
                }
                IssueAction::Create { .. } => {
                    let issue: models::Issue = decode(value)?;
                    render::issue_created(&issue)
                }
                IssueAction::Update { .. } => {
                    let issue: models::Issue = decode(value)?;
                    render::issue_updated(&issue)
                }
            },
            DataCommand::Project { action } => match action {
                ProjectAction::List => {
                    let projects: Vec<models::Project> = decode(value)?;
                    render::project_list(&projects)
                }
                ProjectAction::Get { .. } => {
                    let project: models::Project = decode(value)?;
                    render::project_detail(&project)
                }
                ProjectAction::Create { .. } => {
                    let project: models::Project = decode(value)?;
                    render::project_created(&project)
                }
                ProjectAction::Update { .. } => {
                    let project: models::Project = decode(value)?;
                    render::project_updated(&project)
                }
            },
            DataCommand::Page { action } => match action {
                PageAction::List { .. } => {
                    let pages: Vec<models::Page> = decode(value)?;
                    render::page_list(&pages)
                }
                PageAction::Get { .. } => {
                    let page: models::Page = decode(value)?;
                    render::page_detail(&page)
                }
                PageAction::Create { .. } => {
                    let page: models::Page = decode(value)?;
                    render::page_created(&page)
                }
                PageAction::Update { .. } => {
                    let page: models::Page = decode(value)?;
                    render::page_updated(&page)
                }
            },
            DataCommand::Search { .. } => {
                let results: Vec<models::SearchResult> = decode(value)?;
                render::search_results(&results)
            }
            DataCommand::Comment { action } => match action {
                CommentAction::List { identifier, .. } => {
                    let comments: Vec<models::Comment> = decode(value)?;
                    let continuation = self.continuation?;
                    render::comment_list(&comments, identifier, continuation)
                }
                CommentAction::Add { identifier, .. } => {
                    let comment: models::Comment = decode(value)?;
                    render::comment_added(&comment, identifier)
                }
            },
            DataCommand::Module { action } => match action {
                ModuleAction::List { project } => {
                    let modules: Vec<models::Module> = decode(value)?;
                    render::module_list(&modules, project)
                }
                ModuleAction::Create { project, .. } => {
                    let module: models::Module = decode(value)?;
                    render::module_created(&module, project)
                }
                ModuleAction::Update { .. } => {
                    let module: models::Module = decode(value)?;
                    render::module_updated(&module)
                }
                ModuleAction::Delete { name, .. } => render::module_deleted(name),
            },
            DataCommand::Label { action } => match action {
                LabelAction::List { project } => {
                    let labels: Vec<models::Label> = decode(value)?;
                    render::label_list(&labels, project)
                }
                LabelAction::Create { .. } => {
                    let label: models::Label = decode(value)?;
                    render::label_created(&label)
                }
                LabelAction::Update { .. } => {
                    let label: models::Label = decode(value)?;
                    render::label_updated(&label)
                }
                LabelAction::Delete { name, .. } => render::label_deleted(name),
            },
            DataCommand::Folder { action } => match action {
                FolderAction::List { project } => {
                    let folders: Vec<models::Folder> = decode(value)?;
                    render::folder_list(&folders, project)
                }
                FolderAction::Create { .. } => {
                    let folder: models::Folder = decode(value)?;
                    render::folder_created(&folder)
                }
                FolderAction::Update { name, .. } => {
                    let folder: models::Folder = decode(value)?;
                    render::folder_updated(name, &folder)
                }
                FolderAction::Delete { name, .. } => render::folder_deleted(name),
            },
            DataCommand::Export { action } => {
                let output = match action {
                    ExportAction::Issue { output, .. }
                    | ExportAction::Page { output, .. }
                    | ExportAction::Project { output, .. } => output,
                };
                let written: Vec<PathBuf> = decode::<Vec<String>>(value)?
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                render::export_written(&written, output)
            }
            // The document `bind` produces is identical on both backends, so
            // it renders through the same function the SQL executor calls.
            DataCommand::Bind { .. } => crate::cli::bind::human(value),
            // Same document from both backends, so it renders through the same
            // function the SQL executor calls.
            DataCommand::GitHook { .. } => crate::cli::git_hook::human(value),
        })
    }
}

fn decode<T: DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_survive_output_and_write_errors_reach_the_caller() {
        let command = DataCommand::Project {
            action: ProjectAction::List,
        };
        let output = CommandOutput::new(serde_json::json!({"future_field": [1, 2]}));
        for json in [false, true] {
            let mut bytes = Vec::new();
            output.write(&command, json, &mut bytes).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                output.value
            );
            let mut full = std::io::Cursor::new(&mut [][..]);
            assert_eq!(
                output.write(&command, json, &mut full).unwrap_err().kind(),
                std::io::ErrorKind::WriteZero
            );
        }
    }
}
