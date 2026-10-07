//! Browser-locale label ordering. The comparator and stable sort execute in Rust.

pub(super) use super::super::collation::BrowserCollation;
use super::labels_model::{Usage, js_whitespace};
use crate::{db::models::Label, error::LificError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sort {
    Name,
    Usage,
    Newest,
}
impl Sort {
    pub(super) fn parse(value: &str) -> Result<Self, LificError> {
        match value {
            "name" => Ok(Self::Name),
            "usage" => Ok(Self::Usage),
            "newest" => Ok(Self::Newest),
            _ => Err(LificError::BadRequest("invalid label sort".into())),
        }
    }
}
pub(super) fn visible<'a>(
    labels: &'a [Label],
    usage: &BTreeMap<String, Usage>,
    filter: &str,
    sort: Sort,
    settings: &BrowserCollation,
) -> Result<Vec<&'a Label>, LificError> {
    let query = filter.trim_matches(js_whitespace).to_lowercase();
    let mut result = labels
        .iter()
        .filter(|label| query.is_empty() || label.name.to_lowercase().contains(&query))
        .collect::<Vec<_>>();
    // Rust's sort_by is stable, matching Array.sort's equal-comparator order.
    let collator = settings.comparator()?;
    result.sort_by(|a, b| match sort {
        Sort::Name => collator.compare(&a.name, &b.name),
        Sort::Usage => usage
            .get(&b.name)
            .copied()
            .unwrap_or_default()
            .total()
            .cmp(&usage.get(&a.name).copied().unwrap_or_default().total())
            .then_with(|| collator.compare(&a.name, &b.name)),
        Sort::Newest => b.id.cmp(&a.id),
    });
    Ok(result)
}
#[cfg(test)]
#[path = "collation_tests.rs"]
mod tests;
