//! Pinned ProjectForm and IconPicker decisions, independent of the browser.

use std::{collections::BTreeMap, sync::LazyLock};

use serde::Deserialize;

use crate::{db::models::CreateProject, error::LificError};

pub(crate) const COLUMNS: usize = 8;
pub(crate) const ROW_HEIGHT: usize = 36;
pub(crate) const VIEWPORT_HEIGHT: usize = 280;
const OVERSCAN: usize = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Draft {
    pub(crate) name: String,
    pub(crate) identifier: String,
    pub(crate) identifier_touched: bool,
    pub(crate) description: String,
    pub(crate) emoji: String,
    pub(crate) lead: Option<i64>,
    pub(crate) group: Option<i64>,
    pub(crate) error: String,
}

#[cfg(test)]
pub(super) type DraftWire = (
    String,
    String,
    bool,
    String,
    String,
    Option<i64>,
    Option<i64>,
);

impl Draft {
    #[cfg(test)]
    pub(super) fn from_wire(wire: DraftWire) -> Self {
        let (name, identifier, identifier_touched, description, emoji, lead, group) = wire;
        Self {
            name,
            identifier,
            identifier_touched,
            description,
            emoji,
            lead,
            group,
            error: String::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn wire(&self) -> DraftWire {
        (
            self.name.clone(),
            self.identifier.clone(),
            self.identifier_touched,
            self.description.clone(),
            self.emoji.clone(),
            self.lead,
            self.group,
        )
    }

    pub(super) fn input(&self) -> Result<CreateProject, LificError> {
        let name = trim(&self.name);
        let identifier = trim(&self.identifier).to_uppercase();
        if name.is_empty() || identifier.is_empty() {
            return Err(LificError::BadRequest(
                "Enter a project name and identifier.".into(),
            ));
        }
        let emoji = trim(&self.emoji);
        Ok(CreateProject {
            name: name.to_owned(),
            identifier,
            description: trim(&self.description).to_owned(),
            emoji: (!emoji.is_empty()).then(|| emoji.to_owned()),
            lead_user_id: self.lead,
        })
    }
}

// Share the same ECMAScript normalization used by the local runtime controls.
fn trim(value: &str) -> &str {
    super::super::super::runtime::whitespace::trim_ecmascript(value)
}

#[cfg(test)]
pub(crate) fn generated_identifier(name: &str) -> String {
    name.to_uppercase()
        .chars()
        .filter(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
        .take(5)
        .collect()
}

#[cfg(test)]
pub(crate) fn name_changed(name: &str, identifier: &str, identifier_touched: bool) -> String {
    if identifier_touched || name.is_empty() {
        identifier.to_owned()
    } else {
        generated_identifier(name)
    }
}

#[cfg(test)]
pub(crate) fn preview(identifier: &str) -> String {
    let prefix = trim(identifier).to_uppercase();
    format!("{}-1", if prefix.is_empty() { "PRO" } else { &prefix })
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct EmojiOption {
    pub(crate) value: String,
    pub(crate) name: String,
    pub(crate) group: String,
}

static EMOJIS: LazyLock<Vec<EmojiOption>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("assets/project-picker-emoji.json"))
        .expect("pinned emoji picker metadata is valid")
});
static ICON_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let data: BTreeMap<String, String> =
        serde_json::from_str(include_str!("../assets/project-icons.inline.json"))
            .expect("SVGO-optimized project icon names and bodies are valid");
    data.into_keys().collect()
});

pub(crate) fn emojis(query: &str) -> Vec<EmojiOption> {
    let lowered = query.to_lowercase();
    EMOJIS
        .iter()
        .filter(|choice| trim(query).is_empty() || choice.name.contains(&lowered))
        .cloned()
        .collect()
}

pub(crate) fn icons(query: &str) -> Vec<String> {
    let lowered = query.to_lowercase();
    ICON_NAMES
        .iter()
        .filter(|name| trim(query).is_empty() || name.to_lowercase().contains(&lowered))
        .cloned()
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GridWindow {
    pub(crate) total_height: usize,
    pub(crate) virtualized: bool,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) offset: usize,
}

pub(crate) fn grid_window(count: usize, scroll_top: usize) -> GridWindow {
    let rows = count.div_ceil(COLUMNS);
    let height = rows.saturating_mul(ROW_HEIGHT);
    if height <= VIEWPORT_HEIGHT {
        return GridWindow {
            total_height: height,
            virtualized: false,
            start: 0,
            end: count,
            offset: 0,
        };
    }
    // A late scroll event may refer to the previous larger result set.
    let scroll_top = scroll_top.min(height.saturating_sub(VIEWPORT_HEIGHT));
    let start_row = (scroll_top / ROW_HEIGHT).saturating_sub(OVERSCAN);
    let end_row = scroll_top
        .saturating_add(VIEWPORT_HEIGHT)
        .div_ceil(ROW_HEIGHT)
        .saturating_add(OVERSCAN)
        .min(rows);
    GridWindow {
        total_height: height,
        virtualized: true,
        start: start_row.saturating_mul(COLUMNS).min(count),
        end: end_row.saturating_mul(COLUMNS).min(count),
        offset: start_row.saturating_mul(ROW_HEIGHT),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TriggerRect {
    pub(crate) top: f64,
    pub(crate) bottom: f64,
    pub(crate) left: f64,
    pub(crate) width: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MenuSize {
    pub(crate) height: f64,
    pub(crate) width: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) height: f64,
    pub(crate) width: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MenuGeometry {
    pub(crate) trigger: TriggerRect,
    pub(crate) menu: MenuSize,
    pub(crate) viewport: Viewport,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MenuPosition {
    pub(crate) top: f64,
    pub(crate) left: f64,
    pub(crate) width: f64,
}

#[cfg(test)]
pub(crate) fn menu_position(geometry: MenuGeometry) -> MenuPosition {
    let MenuGeometry {
        trigger,
        menu,
        viewport,
    } = geometry;
    let below = trigger.bottom + 4.0;
    let above = trigger.top - menu.height - 4.0;
    let top = if below + menu.height > viewport.height - 8.0 && above >= 8.0 {
        above
    } else {
        below
    };
    let left = trigger.left.min(viewport.width - menu.width - 8.0).max(8.0);
    MenuPosition {
        top,
        left,
        width: trigger.width,
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
