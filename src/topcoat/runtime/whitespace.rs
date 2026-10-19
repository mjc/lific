//! ECMAScript whitespace for interfaces preserving String.prototype.trim behavior.
use topcoat::runtime::{StrSurrogate, StringSurrogate, Surrogate, Surrogated};

pub(crate) trait StrEcmaTrimExt {
    fn trim_ecmascript(&self) -> StringSurrogate;
}
impl StrEcmaTrimExt for StrSurrogate {
    fn trim_ecmascript(&self) -> StringSurrogate {
        trim_ecmascript(Surrogate::into_real(self))
            .to_owned()
            .into_surrogate()
    }
}

/// Host counterpart for services and models preserving browser trim semantics.
pub(crate) fn trim_ecmascript(text: &str) -> &str {
    text.trim_matches(is_ecmascript_whitespace)
}

pub(crate) fn is_ecmascript_whitespace(scalar: char) -> bool {
    matches!(scalar,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

#[cfg(test)]
#[path = "whitespace_tests.rs"]
mod tests;
