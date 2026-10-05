//! Browser-locale label ordering. The comparator and stable sort execute in Rust.
use super::labels_model::{Usage, js_whitespace};
use crate::{db::models::Label, error::LificError};
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences,
    options::{AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength},
    preferences::{CollationCaseFirst, CollationNumericOrdering},
};
use icu_locale_core::Locale;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum UsageOption {
    Sort,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Sensitivity {
    Base,
    Accent,
    Case,
    Variant,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum CaseFirst {
    False,
    Lower,
    Upper,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BrowserCollation {
    pub(super) locale: String,
    pub(super) usage: UsageOption,
    pub(super) sensitivity: Sensitivity,
    pub(super) ignore_punctuation: bool,
    pub(super) collation: String,
    pub(super) numeric: bool,
    pub(super) case_first: CaseFirst,
}
impl BrowserCollation {
    pub(super) fn from_wire(value: &str) -> Result<Self, LificError> {
        // This is display configuration, never authority. Bound malformed input.
        if value.len() > 2048 {
            return Err(LificError::BadRequest(
                "invalid label collation settings".into(),
            ));
        }
        serde_json::from_str(value)
            .map_err(|_| LificError::BadRequest("invalid label collation settings".into()))
    }
    pub(super) fn comparator(&self) -> Result<CollatorBorrowed<'static>, LificError> {
        let locale: Locale = self
            .locale
            .parse()
            .map_err(|_| LificError::BadRequest("invalid label collation locale".into()))?;
        // The resolved locale can retain Unicode extensions. The resolved
        // numeric/case options below are authoritative; collation gets its own
        // parsed BCP-47 value instead of a handwritten list of supported cultures.
        let locale = if self.collation == "default" {
            locale
        } else {
            format!("{}-u-co-{}", locale.id, self.collation)
                .parse::<Locale>()
                .map_err(|_| LificError::BadRequest("invalid label collation type".into()))?
        };
        let mut preferences = CollatorPreferences::from_locale_strict(&locale)
            .map_err(|_| LificError::BadRequest("invalid label collation preferences".into()))?;
        preferences.case_first = Some(match self.case_first {
            CaseFirst::False => CollationCaseFirst::False,
            CaseFirst::Lower => CollationCaseFirst::Lower,
            CaseFirst::Upper => CollationCaseFirst::Upper,
        });
        preferences.numeric_ordering = Some(if self.numeric {
            CollationNumericOrdering::True
        } else {
            CollationNumericOrdering::False
        });
        let mut options = CollatorOptions::default();
        options.strength = Some(match self.sensitivity {
            Sensitivity::Base | Sensitivity::Case => Strength::Primary,
            Sensitivity::Accent => Strength::Secondary,
            Sensitivity::Variant => Strength::Tertiary,
        });
        options.case_level = Some(if matches!(self.sensitivity, Sensitivity::Case) {
            CaseLevel::On
        } else {
            CaseLevel::Off
        });
        options.alternate_handling = Some(if self.ignore_punctuation {
            AlternateHandling::Shifted
        } else {
            AlternateHandling::NonIgnorable
        });
        options.max_variable = Some(MaxVariable::Punctuation);
        Collator::try_new(preferences, options).map_err(|error| {
            LificError::Internal(format!("label collation data unavailable: {error}"))
        })
    }
}
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
