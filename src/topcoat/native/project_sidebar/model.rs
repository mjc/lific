//! Sidebar decisions shared by desktop and phone wrappers.

use super::super::icons::UiIcon;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Project {
    pub(crate) id: i64,
    pub(crate) identifier: String,
    pub(crate) name: String,
    pub(crate) emoji: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Group {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) project_ids: Vec<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Catalog {
    pub(crate) owner: i64,
    pub(crate) generation: u64,
    pub(crate) groups: Vec<Group>,
    pub(crate) projects: Vec<Project>,
}
impl Catalog {
    pub(crate) fn project_ids(&self) -> Vec<i64> {
        self.projects.iter().map(|project| project.id).collect()
    }
    pub(crate) fn group_ids(&self) -> Vec<i64> {
        self.groups.iter().map(|group| group.id).collect()
    }
    pub(crate) fn containing(&self, project: i64) -> Option<i64> {
        self.groups
            .iter()
            .find(|group| group.project_ids.contains(&project))
            .map(|group| group.id)
    }
    pub(crate) fn siblings(&self, group: Option<i64>) -> Vec<i64> {
        self.projects
            .iter()
            .filter(|project| self.containing(project.id) == group)
            .map(|project| project.id)
            .collect()
    }
    pub(crate) fn rows(&self) -> Vec<(Option<i64>, i64)> {
        self.groups
            .iter()
            .flat_map(|group| {
                self.siblings(Some(group.id))
                    .into_iter()
                    .map(move |id| (Some(group.id), id))
            })
            .chain(self.siblings(None).into_iter().map(|id| (None, id)))
            .collect()
    }
    pub(crate) fn normalize(mut self) -> Self {
        let visible: BTreeSet<_> = self.project_ids().into_iter().collect();
        let mut assigned = BTreeSet::new();
        for group in &mut self.groups {
            group
                .project_ids
                .retain(|id| visible.contains(id) && assigned.insert(*id));
        }
        self
    }
}
impl From<&crate::services::project_sidebar::Catalog> for Catalog {
    fn from(rows: &crate::services::project_sidebar::Catalog) -> Self {
        Self {
            owner: rows.user.id,
            generation: 0,
            projects: rows
                .projects
                .iter()
                .map(|p| Project {
                    id: p.id,
                    identifier: p.identifier.clone(),
                    name: p.name.clone(),
                    emoji: p.emoji.clone(),
                })
                .collect(),
            groups: rows
                .groups
                .iter()
                .map(|g| Group {
                    id: g.id,
                    name: g.name.clone(),
                    project_ids: g.project_ids.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
}
pub(crate) fn move_project(catalog: &Catalog, id: i64, direction: Direction) -> Option<Vec<i64>> {
    let siblings = catalog.siblings(catalog.containing(id));
    let at = siblings.iter().position(|item| *item == id)?;
    let target = match direction {
        Direction::Up => at.checked_sub(1)?,
        Direction::Down => at.checked_add(1)?,
    };
    let other = *siblings.get(target)?;
    let mut ids = catalog.project_ids();
    let a = ids.iter().position(|item| *item == id)?;
    let b = ids.iter().position(|item| *item == other)?;
    ids.swap(a, b);
    Some(ids)
}
pub(crate) fn move_group(catalog: &Catalog, id: i64, direction: Direction) -> Option<Vec<i64>> {
    let mut ids = catalog.group_ids();
    let at = ids.iter().position(|item| *item == id)?;
    let target = match direction {
        Direction::Up => at.checked_sub(1)?,
        Direction::Down => at.checked_add(1)?,
    };
    ids.get(target)?;
    ids.swap(at, target);
    Some(ids)
}

/// Master drag insertion anchors against a visible ungrouped neighbor. It does
/// not rewrite outside-zone positions using the older parked merge algorithm.
pub(crate) fn drag_order(catalog: &Catalog, moved: i64, zone: &[i64]) -> Option<Vec<i64>> {
    let expected: BTreeSet<_> = catalog.siblings(None).into_iter().collect();
    if catalog.containing(moved).is_some()
        || !expected.contains(&moved)
        || zone.len() != expected.len()
        || zone.iter().copied().collect::<BTreeSet<_>>() != expected
    {
        return None;
    }
    let at = zone.iter().position(|id| *id == moved)?;
    let mut rest: Vec<_> = catalog
        .project_ids()
        .into_iter()
        .filter(|id| *id != moved)
        .collect();
    let insertion = if let Some(after) = zone.get(at + 1) {
        rest.iter().position(|id| id == after)?
    } else if let Some(before) = at.checked_sub(1).and_then(|at| zone.get(at)) {
        rest.iter().position(|id| id == before)? + 1
    } else {
        rest.len()
    };
    rest.insert(insertion, moved);
    Some(rest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Token {
    epoch: u64,
    request: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Operation {
    token: Token,
    previous: Catalog,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum EditTarget {
    New { project: Option<i64> },
    Existing(i64),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Edit {
    pub(crate) target: EditTarget,
    pub(crate) draft: String,
    pub(crate) dirty: bool,
    pub(crate) error: String,
    pub(crate) saving: bool,
    pub(crate) return_focus: String,
    save: Option<Token>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Completion {
    Refresh,
    Save,
    Order,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct State {
    pub(crate) catalog: Catalog,
    pub(crate) expanded: BTreeSet<i64>,
    pub(crate) collapsed_groups: BTreeSet<i64>,
    pub(crate) revealed: Option<i64>,
    pub(crate) edit: Option<Edit>,
    pub(crate) error: String,
    pub(crate) dragging: bool,
    pub(crate) groups_ready: bool,
    epoch: u64,
    request: u64,
    pending: Option<Operation>,
    connected: bool,
}
impl State {
    pub(crate) fn admits_completion(&self, token: Token, kind: Completion) -> bool {
        if !self.connected || token.epoch != self.epoch {
            return false;
        }
        match kind {
            Completion::Refresh => {
                token.request == self.request && !self.dragging && !self.pending()
            }
            Completion::Save => self.save_token() == Some(token),
            Completion::Order => self
                .pending
                .as_ref()
                .is_some_and(|operation| operation.token == token),
        }
    }
    pub(crate) fn new(catalog: Catalog) -> Self {
        Self {
            catalog: catalog.normalize(),
            expanded: BTreeSet::new(),
            collapsed_groups: BTreeSet::new(),
            revealed: None,
            edit: None,
            error: String::new(),
            dragging: false,
            groups_ready: true,
            epoch: 0,
            request: 0,
            pending: None,
            connected: true,
        }
    }
    pub(crate) fn begin_refresh(&mut self) -> Option<Token> {
        if !self.connected || self.dragging || self.pending() {
            return None;
        }
        self.request += 1;
        Some(Token {
            epoch: self.epoch,
            request: self.request,
        })
    }
    pub(crate) fn complete_refresh(&mut self, token: Token, catalog: Catalog) -> bool {
        if !self.admits_completion(token, Completion::Refresh) {
            return false;
        }
        self.accept(catalog)
    }
    pub(crate) fn accept_partial(&mut self, mut catalog: Catalog, groups_ready: bool) -> bool {
        if !groups_ready {
            catalog.groups = self.catalog.groups.clone();
        }
        if self.accept(catalog) {
            self.groups_ready = groups_ready;
            true
        } else {
            false
        }
    }
    pub(crate) fn accept(&mut self, catalog: Catalog) -> bool {
        if !self.connected
            || catalog.owner != self.catalog.owner
            || catalog.generation <= self.catalog.generation
        {
            return false;
        }
        let catalog = catalog.normalize();
        if let Some(Edit {
            target: EditTarget::Existing(id),
            draft,
            dirty: false,
            ..
        }) = &mut self.edit
            && let Some(group) = catalog.groups.iter().find(|group| group.id == *id)
        {
            *draft = group.name.clone();
        }
        self.catalog = catalog;
        self.groups_ready = true;
        true
    }
    pub(crate) fn begin_order(
        &mut self,
        projects: Option<&[i64]>,
        groups: Option<&[i64]>,
    ) -> Option<Token> {
        if !self.connected || self.pending() {
            return None;
        }
        let mut next = self.catalog.clone();
        if let Some(ids) = projects {
            if !permutation(&next.project_ids(), ids) {
                return None;
            }
            next.projects = ids
                .iter()
                .filter_map(|id| self.catalog.projects.iter().find(|p| p.id == *id).cloned())
                .collect();
        }
        if let Some(ids) = groups {
            if !permutation(&next.group_ids(), ids) {
                return None;
            }
            next.groups = ids
                .iter()
                .filter_map(|id| self.catalog.groups.iter().find(|g| g.id == *id).cloned())
                .collect();
        }
        self.request += 1;
        let token = Token {
            epoch: self.epoch,
            request: self.request,
        };
        self.pending = Some(Operation {
            token,
            previous: self.catalog.clone(),
        });
        self.catalog = next;
        self.error.clear();
        Some(token)
    }
    pub(crate) fn finish_order(&mut self, token: Token, result: Result<Catalog, String>) -> bool {
        if !self.admits_completion(token, Completion::Order) {
            return false;
        }
        let operation = self.pending.take().expect("checked pending operation");
        match result {
            Ok(catalog) => {
                self.accept(catalog);
            }
            Err(error) => {
                if self.catalog.generation == operation.previous.generation {
                    self.catalog = operation.previous;
                }
                self.error = error;
            }
        }
        self.dragging = false;
        true
    }
    pub(crate) fn reveal(&mut self, identifier: Option<&str>) -> Option<i64> {
        let current = identifier
            .and_then(|ident| {
                self.catalog
                    .projects
                    .iter()
                    .find(|p| p.identifier.eq_ignore_ascii_case(ident))
            })
            .map(|p| p.id);
        let Some(id) = current else {
            self.revealed = None;
            return None;
        };
        if !self.groups_ready || self.revealed == Some(id) {
            return None;
        }
        self.revealed = Some(id);
        self.expanded.insert(id);
        if let Some(group) = self.catalog.containing(id) {
            self.collapsed_groups.remove(&group);
        }
        Some(id)
    }
    pub(crate) fn toggle_project(&mut self, id: i64) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }
    pub(crate) fn toggle_group(&mut self, id: i64) {
        if !self.collapsed_groups.remove(&id) {
            self.collapsed_groups.insert(id);
        }
    }
    pub(crate) fn begin_edit(&mut self, target: EditTarget, return_focus: String) -> bool {
        if self.edit.as_ref().is_some_and(|edit| edit.saving) {
            return false;
        }
        let draft = match target {
            EditTarget::Existing(id) => {
                let Some(group) = self.catalog.groups.iter().find(|g| g.id == id) else {
                    return false;
                };
                group.name.clone()
            }
            EditTarget::New { .. } => String::new(),
        };
        self.edit = Some(Edit {
            target,
            draft,
            dirty: false,
            error: String::new(),
            saving: false,
            return_focus,
            save: None,
        });
        true
    }
    pub(crate) fn draft(&mut self, value: String) {
        if let Some(edit) = &mut self.edit
            && !edit.saving
        {
            edit.draft = value;
            edit.dirty = true;
        }
    }
    pub(crate) fn begin_save(&mut self) -> Option<(EditTarget, String)> {
        if !self.connected || self.dragging || self.pending() {
            return None;
        }
        let edit = self.edit.as_mut()?;
        if edit.saving {
            return None;
        }
        let name = edit
            .draft
            .trim_matches(super::super::super::runtime::whitespace::is_ecmascript_whitespace)
            .to_owned();
        if name.is_empty() {
            edit.error = "Enter a group name.".into();
            return None;
        }
        edit.error.clear();
        edit.saving = true;
        self.request += 1;
        edit.save = Some(Token {
            epoch: self.epoch,
            request: self.request,
        });
        Some((edit.target.clone(), name))
    }
    pub(crate) fn save_token(&self) -> Option<Token> {
        self.edit
            .as_ref()
            .filter(|edit| edit.saving)
            .and_then(|edit| edit.save)
    }
    pub(crate) fn finish_save_token(
        &mut self,
        token: Token,
        result: Result<(), String>,
    ) -> Option<String> {
        if !self.admits_completion(token, Completion::Save) {
            return None;
        }
        self.finish_save(result)
    }
    fn finish_save(&mut self, result: Result<(), String>) -> Option<String> {
        let edit = self.edit.as_mut()?;
        edit.saving = false;
        edit.save = None;
        match result {
            Ok(()) => self.cancel_edit(),
            Err(error) => {
                edit.error = error;
                None
            }
        }
    }
    pub(crate) fn cancel_edit(&mut self) -> Option<String> {
        if self.edit.as_ref().is_some_and(|edit| edit.saving) {
            return None;
        }
        self.edit.take().map(|edit| edit.return_focus)
    }
    pub(crate) fn reset_owner(&mut self, catalog: Catalog) {
        self.epoch += 1;
        self.request += 1;
        self.catalog = catalog.normalize();
        self.expanded.clear();
        self.collapsed_groups.clear();
        self.revealed = None;
        self.edit = None;
        self.error.clear();
        self.dragging = false;
        self.pending = None;
        self.groups_ready = true;
        self.connected = true;
    }
    pub(crate) fn disconnect(&mut self) {
        self.epoch += 1;
        self.request += 1;
        self.pending = None;
        self.dragging = false;
        self.connected = false;
        if let Some(edit) = &mut self.edit {
            edit.saving = false;
            edit.save = None;
        }
    }
    pub(crate) fn valid_envelope(&self) -> bool {
        self.epoch < u64::MAX - 8
            && self.request < u64::MAX - 8
            && self.catalog.generation < u64::MAX - 8
    }
    pub(crate) fn pending(&self) -> bool {
        self.pending.is_some() || self.edit.as_ref().is_some_and(|edit| edit.saving)
    }
}
fn permutation(current: &[i64], submitted: &[i64]) -> bool {
    current.len() == submitted.len()
        && current.iter().copied().collect::<BTreeSet<_>>()
            == submitted.iter().copied().collect::<BTreeSet<_>>()
        && submitted.iter().copied().collect::<BTreeSet<_>>().len() == submitted.len()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sizing {
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) width: f64,
}
pub(crate) fn sizing(preference: Option<f64>, root_font: f64) -> Sizing {
    let scale = if root_font.is_finite() && root_font > 0.0 {
        root_font / 16.0
    } else {
        1.0
    };
    let min = 180.0 * scale.max(1.0);
    let max = 400.0_f64.max(min);
    let preferred = preference
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(180.0, 400.0));
    Sizing {
        min,
        max,
        width: preferred.unwrap_or(230.0 * scale).clamp(min, max),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    Overview,
    Issues,
    Board,
    Graph,
    Modules,
    Pages,
    Files,
    Plans,
    Activity,
    Insights,
}
pub(crate) const DESTINATIONS: [Destination; 10] = [
    Destination::Overview,
    Destination::Issues,
    Destination::Board,
    Destination::Graph,
    Destination::Modules,
    Destination::Pages,
    Destination::Files,
    Destination::Plans,
    Destination::Activity,
    Destination::Insights,
];
impl Destination {
    pub(crate) fn row(self) -> (&'static str, &'static str, UiIcon) {
        match self {
            Self::Overview => ("overview", "Overview", UiIcon::Overview),
            Self::Issues => ("issues", "Issues", UiIcon::Issues),
            Self::Board => ("board", "Board", UiIcon::Board),
            Self::Graph => ("graph", "Graph", UiIcon::Graph),
            Self::Modules => ("modules", "Modules", UiIcon::Modules),
            Self::Pages => ("pages", "Pages", UiIcon::Pages),
            Self::Files => ("files", "Files", UiIcon::Files),
            Self::Plans => ("plans", "Plans", UiIcon::Plans),
            Self::Activity => ("activity", "Activity", UiIcon::Activity),
            Self::Insights => ("insights", "Insights", UiIcon::Insights),
        }
    }
    pub(crate) fn active(self, identifier: &str, path: &str) -> bool {
        let (slug, _, _) = self.row();
        let prefix = format!("/{identifier}/{slug}");
        if self == Self::Overview && path.eq_ignore_ascii_case(&format!("/{identifier}/settings")) {
            return true;
        }
        path.eq_ignore_ascii_case(&prefix)
            || path
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(&prefix))
                && path.as_bytes().get(prefix.len()) == Some(&b'/')
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
