//! Browser-locale ordering shared by native pages. Comparison executes in Rust.

use crate::error::LificError;
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences,
    options::{AlternateHandling, CaseLevel, CollatorOptions, MaxVariable, Strength},
    preferences::{CollationCaseFirst, CollationNumericOrdering},
};
use icu_locale_core::Locale;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UsageOption {
    Sort,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Sensitivity {
    Base,
    Accent,
    Case,
    Variant,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CaseFirst {
    False,
    Lower,
    Upper,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserCollation {
    pub(crate) locale: String,
    pub(crate) usage: UsageOption,
    pub(crate) sensitivity: Sensitivity,
    pub(crate) ignore_punctuation: bool,
    pub(crate) collation: String,
    pub(crate) numeric: bool,
    pub(crate) case_first: CaseFirst,
}
impl BrowserCollation {
    pub(crate) fn from_wire(value: &str) -> Result<Self, LificError> {
        // This is display configuration, never authority. Bound malformed input.
        if value.len() > 2048 {
            return Err(LificError::BadRequest(
                "invalid label collation settings".into(),
            ));
        }
        serde_json::from_str(value)
            .map_err(|_| LificError::BadRequest("invalid label collation settings".into()))
    }
    pub(crate) fn comparator(&self) -> Result<CollatorBorrowed<'static>, LificError> {
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
