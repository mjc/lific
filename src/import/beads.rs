//! Beads JSONL importer (LIF-111).
//!
//! Beads' JSONL export is the only supported input. This module deliberately
//! does not know about Beads' database or executable; it parses a complete
//! snapshot, maps it to the shared import shape, and leaves writes to the
//! existing importer spine.

#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs::File;
#[cfg(test)]
use std::io::Write;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;

use super::{NormalizedComment, NormalizedIssue, NormalizedLabel};
use crate::db::models::{Priority, Status};

#[derive(Debug, thiserror::Error)]
pub enum BeadsImportError {
    #[error("failed to read Beads JSONL: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Beads JSONL at line {line}: {source}")]
    Json {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("Beads JSONL is empty")]
    Empty,
    #[error("Beads issue at line {line} has no id")]
    MissingId { line: usize },
    #[error("Beads issue '{id}' at line {line} has an empty title")]
    EmptyTitle { line: usize, id: String },
    #[error("duplicate Beads issue id '{id}' at line {line}")]
    DuplicateId { line: usize, id: String },
    #[error("Beads issue '{id}' has priority {priority}; expected 0 through 4")]
    InvalidPriority { id: String, priority: i64 },
    #[error("Beads issue '{id}' has a missing dependency endpoint")]
    MissingDependencyEndpoint { id: String },
    #[error(
        "Beads directory '{}' has no issues.jsonl; export/flush the store first or pass the exported file directly",
        path.display()
    )]
    MissingExport { path: PathBuf },
    #[error("Beads source name must be non-empty and contain no control characters")]
    InvalidSource,
}

/// The fields used by the importer. Unknown fields are intentionally ignored
/// so newer Beads exports remain readable without copying arbitrary payloads.
#[derive(Debug, Clone, Deserialize)]
pub struct BeadsIssue {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub design: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub status: String,
    pub priority: i64,
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub comments: Vec<BeadsComment>,
    #[serde(default)]
    pub dependencies: Vec<BeadsDependency>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub closed_by: Option<String>,
    #[serde(default)]
    pub deleted_by: Option<String>,
    #[serde(default)]
    pub estimate: Option<f64>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub defer_until: Option<String>,
    #[serde(default)]
    pub close_reason: Option<String>,
    #[serde(default)]
    pub external_reference: Option<String>,
    #[serde(default)]
    pub source_system: Option<String>,
    #[serde(default, alias = "source_repo")]
    pub source_repository: Option<String>,
    #[serde(default)]
    pub agent_context: Option<serde_json::Value>,
    #[serde(default)]
    pub is_template: bool,
    #[serde(default)]
    pub ephemeral: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeadsComment {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, alias = "created_by")]
    pub author: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default, alias = "body")]
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeadsDependency {
    #[serde(default)]
    pub issue_id: String,
    #[serde(default)]
    pub depends_on_id: String,
    #[serde(rename = "type", default)]
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationType {
    Blocks,
    Related,
    Duplicates,
    Unsupported(String),
}

impl RelationType {
    fn parse(value: &str) -> Self {
        match value {
            "blocks" => Self::Blocks,
            "related" | "relates-to" => Self::Related,
            "duplicates" => Self::Duplicates,
            other => Self::Unsupported(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BeadsRelation {
    pub issue_id: String,
    pub depends_on_id: String,
    pub relation_type: RelationType,
}

#[derive(Debug, Default)]
pub struct BeadsSnapshot {
    pub issues: Vec<BeadsIssue>,
    pub relations: Vec<BeadsRelation>,
}

/// Resolve either a direct JSONL path or the exact `.beads/issues.jsonl`
/// directory layout. No recursive search or database inspection is allowed.
pub fn resolve_jsonl_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("issues.jsonl")
    } else {
        path.to_path_buf()
    }
}

/// Read, normalize, and collect a Beads export without touching the database.
pub fn collect(path: &Path, source: &str) -> Result<super::FetchedIssues, BeadsImportError> {
    if source.trim().is_empty() || source.chars().any(char::is_control) {
        return Err(BeadsImportError::InvalidSource);
    }
    let resolved = resolve_jsonl_path(path);
    if path.is_dir() && !resolved.is_file() {
        return Err(BeadsImportError::MissingExport { path: resolved });
    }
    let file = File::open(&resolved)?;
    let snapshot = parse_jsonl(file)?;
    let mut out = super::FetchedIssues::default();
    for issue in &snapshot.issues {
        if issue.status == "tombstone" {
            out.skipped_tombstones += 1;
        } else if issue.is_template {
            out.skipped_templates += 1;
        } else if issue.ephemeral {
            out.skipped_ephemeral += 1;
        } else {
            out.issues.push(map_issue(issue, source)?);
        }
    }
    for relation in snapshot.relations {
        let (source_id, target_id, relation_type) = match relation.relation_type {
            RelationType::Blocks => (
                relation.depends_on_id,
                relation.issue_id,
                "blocks".to_owned(),
            ),
            RelationType::Related => (
                relation.issue_id,
                relation.depends_on_id,
                "relates_to".to_owned(),
            ),
            RelationType::Duplicates => (
                relation.issue_id,
                relation.depends_on_id,
                "duplicate".to_owned(),
            ),
            RelationType::Unsupported(raw) => (relation.issue_id, relation.depends_on_id, raw),
        };
        out.relations.push(super::NormalizedRelation {
            source: format!("beads:{source}:{source_id}"),
            target: format!("beads:{source}:{target_id}"),
            relation_type,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub enum AnonymizeError {
    #[error(transparent)]
    Import(#[from] BeadsImportError),
    #[error("failed to read anonymization input: {0}")]
    Io(#[from] std::io::Error),
    #[error("anonymization input and output must be different paths")]
    SamePath,
    #[error("anonymization input must be JSONL, not a database or sidecar")]
    DatabaseInput,
    #[error("anonymized output still contains a source value")]
    SourceValueLeaked,
    #[error("failed to serialize anonymized Beads record: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to publish anonymized output: {0}")]
    Publish(String),
}

#[cfg(test)]
#[derive(Default)]
struct Anonymizer {
    issue_ids: BTreeMap<String, String>,
    comment_ids: BTreeMap<String, String>,
    identities: BTreeMap<String, String>,
    labels: BTreeMap<String, String>,
    text_counts: BTreeMap<String, usize>,
    timestamps: BTreeMap<String, String>,
}

#[cfg(test)]
impl Anonymizer {
    fn issue_id(&mut self, raw: &str) -> String {
        if let Some(mapped) = self.issue_ids.get(raw) {
            return mapped.clone();
        }
        let mapped = format!("bead-{:04}", self.issue_ids.len() + 1);
        self.issue_ids.insert(raw.to_owned(), mapped.clone());
        mapped
    }

    fn identity(&mut self, raw: Option<&str>) -> Option<String> {
        raw.filter(|value| !value.is_empty()).map(|value| {
            if let Some(mapped) = self.identities.get(value) {
                return mapped.clone();
            }
            let mapped = format!("user-{:04}", self.identities.len() + 1);
            self.identities.insert(value.to_owned(), mapped.clone());
            mapped
        })
    }

    fn comment_id(&mut self, raw: Option<&str>) -> Option<String> {
        raw.filter(|value| !value.is_empty()).map(|value| {
            if let Some(mapped) = self.comment_ids.get(value) {
                return mapped.clone();
            }
            let mapped = format!("comment-{:04}", self.comment_ids.len() + 1);
            self.comment_ids.insert(value.to_owned(), mapped.clone());
            mapped
        })
    }

    fn label(&mut self, raw: &str) -> String {
        if let Some(mapped) = self.labels.get(raw) {
            return mapped.clone();
        }
        let mapped = format!("label-{:04}", self.labels.len() + 1);
        self.labels.insert(raw.to_owned(), mapped.clone());
        mapped
    }

    fn text(&mut self, field: &str, raw: Option<&str>) -> Option<String> {
        raw.filter(|value| !value.is_empty()).map(|_| {
            let count = self.text_counts.entry(field.to_owned()).or_default();
            *count += 1;
            format!("<{field}-{count:04}>")
        })
    }

    fn timestamp(&self, raw: Option<&str>) -> Option<String> {
        raw.filter(|value| !value.is_empty())
            .and_then(|value| self.timestamps.get(value).cloned())
    }
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct AnonymizedIssue {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    design: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acceptance_criteria: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    status: String,
    priority: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue_type: Option<String>,
    labels: Vec<String>,
    comments: Vec<AnonymizedComment>,
    dependencies: Vec<AnonymizedDependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    closed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    closed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    defer_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_context: Option<serde_json::Value>,
    is_template: bool,
    ephemeral: bool,
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct AnonymizedComment {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    text: String,
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct AnonymizedDependency {
    issue_id: String,
    depends_on_id: String,
    #[serde(rename = "type")]
    relation_type: String,
}

/// Produce deterministic, privacy-safe JSONL from a complete parsed snapshot.
/// The returned string is intentionally the only output surface; mapping state
/// is kept in memory and is never serialized as a sidecar.
#[cfg(test)]
pub fn anonymize_jsonl<R: Read>(reader: R) -> Result<String, AnonymizeError> {
    let snapshot = parse_jsonl(reader)?;
    let source_values = source_values(&snapshot);
    let mut anonymizer = Anonymizer::default();
    for issue in &snapshot.issues {
        anonymizer.issue_id(&issue.id);
    }
    let mut timestamps = snapshot
        .issues
        .iter()
        .flat_map(|issue| {
            [
                issue.started_at.as_deref(),
                issue.created_at.as_deref(),
                issue.updated_at.as_deref(),
                issue.closed_at.as_deref(),
                issue.deleted_at.as_deref(),
                issue.due_at.as_deref(),
                issue.defer_until.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(
                issue
                    .comments
                    .iter()
                    .filter_map(|comment| comment.created_at.as_deref()),
            )
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    timestamps.sort();
    timestamps.dedup();
    for (index, timestamp) in timestamps.into_iter().enumerate() {
        let synthetic = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&chrono::Utc)
            + chrono::Duration::seconds(index as i64);
        anonymizer.timestamps.insert(
            timestamp,
            synthetic.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
    }

    let mut lines = Vec::with_capacity(snapshot.issues.len());
    for issue in &snapshot.issues {
        let comments = issue
            .comments
            .iter()
            .map(|comment| AnonymizedComment {
                id: anonymizer.comment_id(comment.id.as_deref()),
                author: anonymizer.identity(comment.author.as_deref()),
                created_at: anonymizer.timestamp(comment.created_at.as_deref()),
                text: anonymizer
                    .text("comment", Some(&comment.text))
                    .unwrap_or_else(|| "<comment-empty>".to_owned()),
            })
            .collect();
        let dependencies = issue
            .dependencies
            .iter()
            .map(|dependency| AnonymizedDependency {
                issue_id: anonymizer.issue_id(&dependency.issue_id),
                depends_on_id: anonymizer.issue_id(&dependency.depends_on_id),
                relation_type: anonymize_relation(&mut anonymizer, &dependency.relation_type),
            })
            .collect();
        let record = AnonymizedIssue {
            id: anonymizer.issue_id(&issue.id),
            title: anonymizer
                .text("title", Some(&issue.title))
                .unwrap_or_else(|| "<title-empty>".to_owned()),
            description: anonymizer.text("description", issue.description.as_deref()),
            design: anonymizer.text("design", issue.design.as_deref()),
            acceptance_criteria: anonymizer
                .text("acceptance", issue.acceptance_criteria.as_deref()),
            notes: anonymizer.text("notes", issue.notes.as_deref()),
            status: anonymize_status(&mut anonymizer, &issue.status),
            priority: issue.priority,
            issue_type: anonymizer.text("type", issue.issue_type.as_deref()),
            labels: issue
                .labels
                .iter()
                .map(|label| anonymizer.label(label))
                .collect(),
            comments,
            dependencies,
            assignee: anonymizer.identity(issue.assignee.as_deref()),
            owner: anonymizer.identity(issue.owner.as_deref()),
            created_by: anonymizer.identity(issue.created_by.as_deref()),
            closed_by: anonymizer.identity(issue.closed_by.as_deref()),
            deleted_by: anonymizer.identity(issue.deleted_by.as_deref()),
            estimate: issue.estimate,
            started_at: anonymizer.timestamp(issue.started_at.as_deref()),
            created_at: anonymizer.timestamp(issue.created_at.as_deref()),
            updated_at: anonymizer.timestamp(issue.updated_at.as_deref()),
            closed_at: anonymizer.timestamp(issue.closed_at.as_deref()),
            deleted_at: anonymizer.timestamp(issue.deleted_at.as_deref()),
            due_at: anonymizer.timestamp(issue.due_at.as_deref()),
            defer_until: anonymizer.timestamp(issue.defer_until.as_deref()),
            close_reason: anonymizer.text("close-reason", issue.close_reason.as_deref()),
            external_reference: anonymizer
                .text("external-reference", issue.external_reference.as_deref()),
            source_system: anonymizer.text("source-system", issue.source_system.as_deref()),
            source_repository: anonymizer
                .text("source-repository", issue.source_repository.as_deref()),
            agent_context: issue
                .agent_context
                .as_ref()
                .map(|_| serde_json::json!({"redacted": true})),
            is_template: issue.is_template,
            ephemeral: issue.ephemeral,
        };
        lines.push(serde_json::to_string(&record)?);
    }
    let output = format!("{}\n", lines.join("\n"));
    for value in source_values {
        if value.len() >= 3 && !is_structural_value(&value) && output.contains(&value) {
            return Err(AnonymizeError::SourceValueLeaked);
        }
    }
    Ok(output)
}

/// An atomic file wrapper for the developer workflow. It refuses databases,
/// sidecars, and in-place transformation, and parses before creating output.
#[cfg(test)]
pub fn anonymize_file(input: &Path, output: &Path) -> Result<(), AnonymizeError> {
    if input == output {
        return Err(AnonymizeError::SamePath);
    }
    if matches!(
        input.extension().and_then(|ext| ext.to_str()),
        Some("db" | "db-wal" | "db-shm")
    ) {
        return Err(AnonymizeError::DatabaseInput);
    }
    let input_bytes = std::fs::read(input)?;
    let contents = anonymize_jsonl(input_bytes.as_slice())?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output)
        .map_err(|error| AnonymizeError::Publish(error.error.to_string()))?;
    Ok(())
}

#[cfg(test)]
fn anonymize_status(anonymizer: &mut Anonymizer, value: &str) -> String {
    match value {
        "open" | "in_progress" | "closed" | "blocked" | "deferred" | "draft" | "pinned"
        | "tombstone" => value.to_owned(),
        _ => anonymizer
            .text("status", Some(value))
            .unwrap_or_else(|| "<status-empty>".to_owned()),
    }
}

#[cfg(test)]
fn anonymize_relation(anonymizer: &mut Anonymizer, value: &str) -> String {
    match value {
        "blocks" | "related" | "relates-to" | "duplicates" => value.to_owned(),
        _ => anonymizer
            .text("relation", Some(value))
            .unwrap_or_else(|| "<relation-empty>".to_owned()),
    }
}

#[cfg(test)]
fn source_values(snapshot: &BeadsSnapshot) -> Vec<String> {
    snapshot
        .issues
        .iter()
        .flat_map(|issue| {
            let mut values = vec![issue.id.clone(), issue.title.clone()];
            values.extend(
                [
                    issue.description.clone(),
                    issue.design.clone(),
                    issue.acceptance_criteria.clone(),
                    issue.notes.clone(),
                    issue.issue_type.clone(),
                    issue.assignee.clone(),
                    issue.owner.clone(),
                    issue.created_by.clone(),
                    issue.closed_by.clone(),
                    issue.deleted_by.clone(),
                    issue.close_reason.clone(),
                    issue.external_reference.clone(),
                    issue.source_system.clone(),
                    issue.source_repository.clone(),
                ]
                .into_iter()
                .flatten(),
            );
            values.extend(issue.labels.iter().cloned());
            for comment in &issue.comments {
                values.extend(
                    [comment.id.clone(), comment.author.clone()]
                        .into_iter()
                        .flatten(),
                );
                values.push(comment.text.clone());
            }
            for dependency in &issue.dependencies {
                values.extend([
                    dependency.issue_id.clone(),
                    dependency.depends_on_id.clone(),
                    dependency.relation_type.clone(),
                ]);
            }
            values
        })
        .collect()
}

#[cfg(test)]
fn is_structural_value(value: &str) -> bool {
    matches!(
        value,
        "open"
            | "in_progress"
            | "closed"
            | "blocked"
            | "deferred"
            | "draft"
            | "pinned"
            | "tombstone"
            | "blocks"
            | "related"
            | "relates-to"
            | "duplicates"
    )
}

/// Parse and validate the complete JSONL snapshot before any database write.
pub fn parse_jsonl<R: Read>(reader: R) -> Result<BeadsSnapshot, BeadsImportError> {
    let mut snapshot = BeadsSnapshot::default();
    let mut ids = HashSet::new();

    for (line_index, line) in BufReader::new(reader).lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("<<<<<<<")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>>")
        {
            return Err(BeadsImportError::Json {
                line: line_number,
                source: serde_json::from_str::<serde_json::Value>(trimmed).unwrap_err(),
            });
        }

        let issue: BeadsIssue =
            serde_json::from_str(trimmed).map_err(|source| BeadsImportError::Json {
                line: line_number,
                source,
            })?;
        if issue.id.trim().is_empty() {
            return Err(BeadsImportError::MissingId { line: line_number });
        }
        if issue.title.trim().is_empty() {
            return Err(BeadsImportError::EmptyTitle {
                line: line_number,
                id: issue.id,
            });
        }
        if !ids.insert(issue.id.clone()) {
            return Err(BeadsImportError::DuplicateId {
                line: line_number,
                id: issue.id,
            });
        }
        if !(0..=4).contains(&issue.priority) {
            return Err(BeadsImportError::InvalidPriority {
                id: issue.id,
                priority: issue.priority,
            });
        }
        for dependency in &issue.dependencies {
            if dependency.issue_id.is_empty() || dependency.depends_on_id.is_empty() {
                return Err(BeadsImportError::MissingDependencyEndpoint {
                    id: issue.id.clone(),
                });
            }
            snapshot.relations.push(BeadsRelation {
                issue_id: dependency.issue_id.clone(),
                depends_on_id: dependency.depends_on_id.clone(),
                relation_type: RelationType::parse(&dependency.relation_type),
            });
        }
        snapshot.issues.push(issue);
    }

    if snapshot.issues.is_empty() {
        return Err(BeadsImportError::Empty);
    }
    Ok(snapshot)
}

/// Map one Beads issue into the shared normalized import shape.
pub fn map_issue(issue: &BeadsIssue, source: &str) -> Result<NormalizedIssue, BeadsImportError> {
    let mut labels = dedupe_labels(&issue.labels);
    let status = map_status(&issue.status, &mut labels);
    let priority = match issue.priority {
        0 => Priority::Urgent,
        1 => Priority::High,
        2 => Priority::Medium,
        3 => Priority::Low,
        4 => Priority::None,
        priority => {
            return Err(BeadsImportError::InvalidPriority {
                id: issue.id.clone(),
                priority,
            });
        }
    };
    if let Some(issue_type) = issue
        .issue_type
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        push_label(&mut labels, format!("beads:type:{issue_type}"));
    }

    let mut raw_comments = issue.comments.iter().collect::<Vec<_>>();
    raw_comments.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let comments = raw_comments
        .iter()
        .map(|comment| NormalizedComment {
            author: comment
                .author
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
            created_at: comment.created_at.clone().filter(|value| !value.is_empty()),
            body: comment.text.clone(),
        })
        .collect::<Vec<_>>();
    Ok(NormalizedIssue {
        source: format!("beads:{source}:{}", issue.id),
        title: issue.title.clone(),
        description: description(issue),
        status,
        priority,
        start_date: issue.defer_until.as_deref().and_then(calendar_date),
        target_date: issue.due_at.as_deref().and_then(calendar_date),
        labels,
        comments,
    })
}

fn map_status(raw: &str, labels: &mut Vec<NormalizedLabel>) -> Status {
    let status = match raw {
        "open" => Status::Backlog,
        "in_progress" => Status::Active,
        "closed" => Status::Done,
        "blocked" | "deferred" | "draft" | "pinned" => Status::Backlog,
        other => {
            if !other.is_empty() {
                push_label(labels, format!("beads:status:{other}"));
            }
            Status::Backlog
        }
    };
    if matches!(raw, "blocked" | "deferred" | "draft" | "pinned") {
        push_label(labels, format!("beads:status:{raw}"));
    }
    status
}

fn dedupe_labels(names: &[String]) -> Vec<NormalizedLabel> {
    let mut seen = HashSet::new();
    names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .map(|name| NormalizedLabel {
            name: name.clone(),
            color: None,
        })
        .collect()
}

fn push_label(labels: &mut Vec<NormalizedLabel>, name: String) {
    if !labels.iter().any(|label| label.name == name) {
        labels.push(NormalizedLabel { name, color: None });
    }
}

fn description(issue: &BeadsIssue) -> String {
    let mut sections = Vec::new();
    if let Some(body) = issue.description.as_deref().filter(|body| !body.is_empty()) {
        sections.push(body.to_owned());
    }
    for (heading, body) in [
        ("## Design", issue.design.as_deref()),
        (
            "## Acceptance criteria",
            issue.acceptance_criteria.as_deref(),
        ),
        ("## Notes", issue.notes.as_deref()),
    ] {
        if let Some(body) = body.filter(|body| !body.is_empty()) {
            sections.push(format!("{heading}\n\n{body}"));
        }
    }
    if let Some(context) = issue.agent_context.as_ref()
        && let Ok(context) = serde_json::to_string_pretty(context)
    {
        sections.push(format!("## Agent context\n\n```json\n{context}\n```"));
    }

    let mut metadata = vec![
        ("Beads ID", Some(issue.id.clone())),
        ("Status", non_empty(&issue.status)),
        (
            "Type",
            issue.issue_type.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Assignee",
            issue.assignee.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Owner",
            issue.owner.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Created by",
            issue.created_by.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Closed by",
            issue.closed_by.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Deleted by",
            issue.deleted_by.clone().filter(|value| !value.is_empty()),
        ),
        ("Estimate", issue.estimate.map(|value| value.to_string())),
        (
            "Started at",
            issue.started_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Due at",
            issue.due_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Defer until",
            issue.defer_until.clone().filter(|value| !value.is_empty()),
        ),
        (
            "External reference",
            issue
                .external_reference
                .clone()
                .filter(|value| !value.is_empty()),
        ),
        (
            "Source system",
            issue
                .source_system
                .clone()
                .filter(|value| !value.is_empty()),
        ),
        (
            "Source repository",
            issue
                .source_repository
                .clone()
                .filter(|value| !value.is_empty()),
        ),
        (
            "Created at",
            issue.created_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Updated at",
            issue.updated_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Closed at",
            issue.closed_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Deleted at",
            issue.deleted_at.clone().filter(|value| !value.is_empty()),
        ),
        (
            "Close reason",
            issue.close_reason.clone().filter(|value| !value.is_empty()),
        ),
    ];
    metadata.retain(|(_, value)| value.is_some());
    if !metadata.is_empty() {
        let rows = metadata
            .into_iter()
            .map(|(name, value)| {
                format!(
                    "| {name} | {} |",
                    value.unwrap_or_default().replace('|', "\\|")
                )
            })
            .collect::<Vec<_>>();
        sections.push(format!(
            "## Imported from Beads\n\n| Field | Value |\n| --- | --- |\n{}",
            rows.join("\n")
        ));
    }
    sections.join("\n\n")
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn calendar_date(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let date = bytes.get(..10)?;
    if date[4] != b'-' || date[7] != b'-' {
        return None;
    }
    if date[..4]
        .iter()
        .chain(&date[5..7])
        .chain(&date[8..10])
        .any(|byte| !byte.is_ascii_digit())
    {
        return None;
    }
    value.get(..10).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn maps_a_jsonl_issue_into_the_shared_import_shape() {
        let input = r#"{"id":"bd-1","title":"Ship it","description":"body","design":"design","acceptance_criteria":"done","notes":"note","status":"in_progress","priority":1,"issue_type":"bug","owner":"alice@example.test","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z","due_at":"2026-01-03T12:00:00Z","defer_until":"2025-12-31T00:00:00Z","labels":["urgent"],"comments":[{"author":"bob","created_at":"2026-01-01T01:00:00Z","text":"comment"}],"dependencies":[{"issue_id":"bd-1","depends_on_id":"bd-2","type":"blocks"}]}"#;

        let snapshot = parse_jsonl(Cursor::new(input)).unwrap();
        let issue = &snapshot.issues[0];
        let normalized = map_issue(issue, "workspace").unwrap();

        assert_eq!(normalized.source, "beads:workspace:bd-1");
        assert_eq!(normalized.status, Status::Active);
        assert_eq!(normalized.priority, Priority::High);
        assert_eq!(normalized.start_date.as_deref(), Some("2025-12-31"));
        assert_eq!(normalized.target_date.as_deref(), Some("2026-01-03"));
        assert!(normalized.description.contains("## Design"));
        assert!(normalized.description.contains("## Acceptance criteria"));
        assert!(normalized.description.contains("## Notes"));
        assert!(normalized.description.contains("## Imported from Beads"));
        assert_eq!(normalized.labels[0].name, "urgent");
        assert_eq!(normalized.comments[0].body, "comment");
        assert_eq!(snapshot.relations[0].relation_type, RelationType::Blocks);
    }

    #[test]
    fn rejects_malformed_later_line_before_returning_a_partial_snapshot() {
        let input = "{\"id\":\"bd-1\",\"title\":\"first\",\"priority\":4}\nnot json\n";

        let error = parse_jsonl(Cursor::new(input)).unwrap_err();
        assert!(matches!(error, BeadsImportError::Json { line: 2, .. }));
    }

    #[test]
    fn rejects_duplicate_ids_and_out_of_range_priorities() {
        let duplicate = concat!(
            r#"{"id":"bd-1","title":"first","priority":4}"#,
            "\n",
            r#"{"id":"bd-1","title":"again","priority":4}"#
        );
        assert!(matches!(
            parse_jsonl(Cursor::new(duplicate)),
            Err(BeadsImportError::DuplicateId { line: 2, .. })
        ));

        let invalid = r#"{"id":"bd-1","title":"bad priority","priority":5}"#;
        assert!(matches!(
            parse_jsonl(Cursor::new(invalid)),
            Err(BeadsImportError::InvalidPriority { priority: 5, .. })
        ));

        let missing = r#"{"id":"bd-1","title":"missing priority"}"#;
        assert!(matches!(
            parse_jsonl(Cursor::new(missing)),
            Err(BeadsImportError::Json { line: 1, .. })
        ));
    }

    #[test]
    fn directory_without_export_fails_before_database_access() {
        let directory = tempfile::tempdir().unwrap();

        let error = collect(directory.path(), "test").unwrap_err();
        assert!(matches!(error, BeadsImportError::MissingExport { .. }));
    }

    #[test]
    fn maps_status_facets_and_deduplicates_labels() {
        let input = r#"{"id":"bd-1","title":"blocked","status":"blocked","priority":4,"issue_type":"bug","labels":["bug","bug"]}"#;
        let issue = &parse_jsonl(Cursor::new(input)).unwrap().issues[0];
        let mapped = map_issue(issue, "test").unwrap();
        let labels = mapped
            .labels
            .iter()
            .map(|label| label.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["bug", "beads:status:blocked", "beads:type:bug"]);
        assert_eq!(mapped.status, Status::Backlog);
    }

    #[test]
    fn malformed_unicode_dates_are_rejected_without_panicking() {
        assert_eq!(calendar_date("0000-01-éé"), None);
    }

    #[test]
    fn skips_tombstones_templates_and_ephemeral_records() {
        let input = concat!(
            r#"{"id":"live","title":"live","status":"open","priority":4}"#,
            "\n",
            r#"{"id":"tombstone","title":"gone","status":"tombstone","priority":4}"#,
            "\n",
            r#"{"id":"template","title":"template","status":"open","priority":4,"is_template":true}"#,
            "\n",
            r#"{"id":"ephemeral","title":"ephemeral","status":"open","priority":4,"ephemeral":true}"#
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), input).unwrap();

        let fetched = collect(file.path(), "test").unwrap();
        assert_eq!(fetched.issues.len(), 1);
        assert_eq!(fetched.skipped_tombstones, 1);
        assert_eq!(fetched.skipped_templates, 1);
        assert_eq!(fetched.skipped_ephemeral, 1);
    }

    #[test]
    fn reverses_blocks_and_keeps_unsupported_relation_types_for_reporting() {
        let input = concat!(
            r#"{"id":"bd-1","title":"blocked","priority":4,"dependencies":[{"issue_id":"bd-1","depends_on_id":"bd-2","type":"blocks"}]}"#,
            "\n",
            r#"{"id":"bd-2","title":"blocker","priority":4,"dependencies":[{"issue_id":"bd-2","depends_on_id":"bd-1","type":"parent-child"}]}"#
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), input).unwrap();
        let fetched = collect(file.path(), "test").unwrap();
        let blocks = fetched
            .relations
            .iter()
            .find(|relation| relation.relation_type == "blocks")
            .unwrap();
        assert_eq!(blocks.source, "beads:test:bd-2");
        assert_eq!(blocks.target, "beads:test:bd-1");
        assert!(
            fetched
                .relations
                .iter()
                .any(|relation| relation.relation_type == "parent-child")
        );
    }

    #[test]
    fn anonymizes_without_leaking_fixture_values() {
        let input = concat!(
            r#"{"id":"secret-123","title":"Deploy from /home/alice/project","description":"alice@example.test https://private.test/repo","status":"open","priority":4,"issue_type":"bug","owner":"alice@example.test","created_by":"creator-secret","closed_by":"closer-secret","deleted_by":"deleter-secret","created_at":"2026-01-01T00:00:00Z","labels":["internal"],"comments":[{"id":"comment-1","author":"alice","created_at":"2026-01-02T00:00:00Z","text":"private phrase"}],"private_extra":"must be dropped","dependencies":[{"issue_id":"secret-123","depends_on_id":"secret-456","type":"blocks"}]}"#,
            "\n",
            r#"{"id":"secret-456","title":"Second","status":"closed","priority":4}"#
        );

        let output = anonymize_jsonl(Cursor::new(input)).unwrap();
        assert!(!output.contains("secret-123"));
        assert!(!output.contains("secret-456"));
        assert!(!output.contains("alice@example.test"));
        assert!(!output.contains("creator-secret"));
        assert!(!output.contains("closer-secret"));
        assert!(!output.contains("deleter-secret"));
        assert!(!output.contains("/home/alice/project"));
        assert!(!output.contains("private phrase"));
        assert!(!output.contains("must be dropped"));
        assert!(output.contains("bead-0001"));
        assert!(output.contains("bead-0002"));
        assert!(output.contains("comment-0001"));
        assert!(output.contains("\"type\":\"blocks\""));
        let anonymized = parse_jsonl(Cursor::new(output)).unwrap();
        assert_eq!(anonymized.relations.len(), 1);
        assert_eq!(anonymized.relations[0].issue_id, "bead-0001");
        assert_eq!(anonymized.relations[0].depends_on_id, "bead-0002");
    }

    #[test]
    fn anonymizer_file_is_atomic_and_can_run_from_explicit_environment_paths() {
        let input = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            input.path(),
            r#"{"id":"secret","title":"Private","status":"open","priority":4}"#,
        )
        .unwrap();
        let output = input.path().with_extension("jsonl");
        anonymize_file(input.path(), &output).unwrap();
        assert!(output.is_file());
        assert!(parse_jsonl(std::fs::File::open(&output).unwrap()).is_ok());

        let malformed = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(malformed.path(), "not json").unwrap();
        let malformed_output = malformed.path().with_extension("out.jsonl");
        assert!(anonymize_file(malformed.path(), &malformed_output).is_err());
        assert!(!malformed_output.exists());

        if let (Ok(env_input), Ok(env_output)) = (
            std::env::var("LIFIC_BEADS_ANON_INPUT"),
            std::env::var("LIFIC_BEADS_ANON_OUTPUT"),
        ) {
            anonymize_file(Path::new(&env_input), Path::new(&env_output)).unwrap();
        }
    }
}
