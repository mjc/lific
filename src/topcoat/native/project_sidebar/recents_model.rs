//! One subordinate recents cache for the shared desktop/phone sidebar owner.
//! This state is presentation only. Native procedures reauthorize each read.
use super::model::Project;
pub(crate) use crate::services::project_recents::{ReadFailure, Row, Section};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Token {
    pub(crate) owner: i64,
    pub(crate) project: (i64, String),
    pub(crate) section: Section,
    pub(crate) epoch: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Focus {
    None,
    Keep(String),
    Disclosure,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct State {
    pub(crate) owner: i64,
    pub(crate) project: Option<(i64, String)>,
    pub(crate) section: Option<Section>,
    pub(crate) visible: bool,
    pub(crate) loading: bool,
    pub(crate) open: bool,
    pub(crate) error: Option<String>,
    cache: [Vec<Row>; 4],
    epoch: u64,
    connected: bool,
}
impl State {
    pub(crate) fn new(owner: i64, open: bool) -> Self {
        Self {
            owner,
            project: None,
            section: None,
            visible: false,
            loading: false,
            open,
            error: None,
            cache: std::array::from_fn(|_| Vec::new()),
            epoch: 0,
            connected: true,
        }
    }
    pub(crate) fn valid_envelope(&self) -> bool {
        let Some((id, project)) = &self.project else {
            return self.cache.iter().all(Vec::is_empty) && !self.visible && !self.loading;
        };
        if self.owner <= 0
            || *id <= 0
            || project.len() > 32
            || !project
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase)
            || !project
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return false;
        }
        [
            Section::Issues,
            Section::Modules,
            Section::Pages,
            Section::Plans,
        ]
        .into_iter()
        .all(|section| {
            let rows = &self.cache[section.index()];
            rows.len() <= 5
                && rows.iter().all(|row| {
                    let prefix = format!("/{project}/{}/", section.as_str());
                    let Some(target) = row.href.strip_prefix(&prefix) else {
                        return false;
                    };
                    if row.label.len() > 16 * 1024 {
                        return false;
                    }
                    match section {
                        Section::Issues => {
                            row.identifier.as_deref() == Some(target)
                                && target.strip_prefix(&format!("{project}-")).is_some_and(
                                    |sequence| sequence.parse::<i64>().is_ok_and(|n| n > 0),
                                )
                        }
                        _ => row.identifier.is_none() && target.parse::<i64>().is_ok_and(|n| n > 0),
                    }
                })
        })
    }
    pub(crate) fn rows(&self) -> &[Row] {
        self.section
            .filter(|_| self.visible)
            .map_or(&[], |section| self.cache[section.index()].as_slice())
    }
    pub(crate) fn begin(
        &mut self,
        owner: i64,
        project: Option<&Project>,
        section: Option<Section>,
        private: bool,
    ) -> Option<Token> {
        self.advance();
        let selected = project
            .filter(|project| private && owner > 0 && project.id > 0)
            .map(|project| (project.id, project.identifier.clone()));
        if owner != self.owner || selected != self.project {
            self.cache = std::array::from_fn(|_| Vec::new());
        }
        self.owner = owner;
        self.project = selected;
        self.section = section.filter(|_| self.project.is_some());
        self.visible = self.connected && self.section.is_some();
        self.loading = self.visible;
        self.error = None;
        if !self.visible {
            return None;
        }
        Some(Token {
            owner,
            project: self.project.clone()?,
            section: self.section?,
            epoch: self.epoch,
        })
    }
    pub(crate) fn finish(
        &mut self,
        token: Token,
        result: Result<Vec<Row>, ReadFailure>,
        focused_href: Option<&str>,
    ) -> Option<Focus> {
        if !self.connected
            || self.epoch != token.epoch
            || self.owner != token.owner
            || self.project.as_ref() != Some(&token.project)
            || self.section != Some(token.section)
        {
            return None;
        }
        self.loading = false;
        match result {
            Ok(rows) => {
                self.cache[token.section.index()] = rows.into_iter().take(5).collect();
                self.error = None;
            }
            Err(error) => {
                if error.access {
                    self.cache = std::array::from_fn(|_| Vec::new());
                }
                self.error = Some(error.message);
            }
        }
        Some(match focused_href {
            Some(href) if self.rows().iter().any(|row| row.href == href) => {
                Focus::Keep(href.into())
            }
            Some(_) => Focus::Disclosure,
            None => Focus::None,
        })
    }
    pub(crate) fn invalidate(&mut self, owner: i64) {
        self.advance();
        self.owner = owner;
        self.project = None;
        self.section = None;
        self.cache = std::array::from_fn(|_| Vec::new());
        self.visible = false;
        self.loading = false;
        self.error = None;
    }
    pub(crate) fn disconnect(&mut self) {
        self.invalidate(self.owner);
        self.connected = false;
    }
    fn advance(&mut self) {
        // At wrap, shut down this owner rather than reusing a token generation.
        if let Some(next) = self.epoch.checked_add(1) {
            self.epoch = next;
        } else {
            self.connected = false;
        }
    }
    pub(crate) fn status(&self) -> String {
        if self.loading {
            self.section.map_or_else(String::new, |section| {
                format!("Loading recent {}…", section.as_str())
            })
        } else {
            String::new()
        }
    }
}
#[cfg(test)]
#[path = "recents_tests.rs"]
mod tests;
