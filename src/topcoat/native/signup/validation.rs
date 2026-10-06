//! Signup.svelte's browser validation, independent of server signup policy.
use std::sync::LazyLock;

use regex::Regex;
use topcoat::runtime::{Expr, expr};

const USERNAME_PATTERN: &str = r"^[a-zA-Z0-9_-]{2,}$";
// ECMAScript \s, shared by Rust regex and the browser RegExp unicode mode.
const EMAIL_PATTERN: &str = r"^[^\u{0009}-\u{000d}\u{0020}\u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}@]+@[^\u{0009}-\u{000d}\u{0020}\u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}@]+\.[^\u{0009}-\u{000d}\u{0020}\u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}@]+$";
const LOWERCASE_PATTERN: &str = "[a-z]";
const UPPERCASE_PATTERN: &str = "[A-Z]";
// JS \d and \W are ASCII classes, including underscore as a word character.
const NUMBER_OR_SYMBOL_PATTERN: &str = "[0-9]|[^a-zA-Z0-9_]";
const MIN_PASSWORD_UNITS: usize = 8;

static USERNAME: LazyLock<Regex> = LazyLock::new(|| pinned_regex(USERNAME_PATTERN));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| pinned_regex(EMAIL_PATTERN));
static LOWERCASE: LazyLock<Regex> = LazyLock::new(|| pinned_regex(LOWERCASE_PATTERN));
static UPPERCASE: LazyLock<Regex> = LazyLock::new(|| pinned_regex(UPPERCASE_PATTERN));
static NUMBER_OR_SYMBOL: LazyLock<Regex> = LazyLock::new(|| pinned_regex(NUMBER_OR_SYMBOL_PATTERN));

fn pinned_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("pinned signup validation expression is valid")
}

fn matches(text: Expr<String>, pattern: &str, matcher: &Regex) -> Expr<bool> {
    let pattern = pattern.to_owned();
    let matched = matcher.is_match(&text.clone().into_evaluated_and_js().0);
    expr!(raw!(
        "cx.hydrate(new RegExp(${pattern}.toString(), 'u').test(${text}.toString()))",
        matched
    ))
}

pub(super) fn username_valid(text: Expr<String>) -> Expr<bool> {
    use super::super::super::runtime::whitespace::StrEcmaTrimExt;
    matches(expr!(text.trim_ecmascript()), USERNAME_PATTERN, &USERNAME)
}

pub(super) fn email_valid(text: Expr<String>) -> Expr<bool> {
    matches(text, EMAIL_PATTERN, &EMAIL)
}

pub(super) fn password_rules(text: Expr<String>) -> (Expr<bool>, Expr<bool>, Expr<bool>) {
    let minimum = MIN_PASSWORD_UNITS;
    let initial_length = text
        .clone()
        .into_evaluated_and_js()
        .0
        .encode_utf16()
        .count();
    let long_enough = expr!(raw!(
        "cx.hydrate(${text}.toString().length >= Number(${minimum}.toString()))",
        initial_length >= minimum
    ));
    let lowercase = matches(text.clone(), LOWERCASE_PATTERN, &LOWERCASE);
    let uppercase = matches(text.clone(), UPPERCASE_PATTERN, &UPPERCASE);
    let mixed_case = expr!(if lowercase { uppercase } else { false });
    let number_or_symbol = matches(text, NUMBER_OR_SYMBOL_PATTERN, &NUMBER_OR_SYMBOL);
    (long_enough, mixed_case, number_or_symbol)
}

#[cfg(test)]
fn username_ok(username: &str) -> bool {
    username_valid(Expr::from(username.to_owned()))
        .into_evaluated_and_js()
        .0
}

#[cfg(test)]
fn email_ok(email: &str) -> bool {
    email_valid(Expr::from(email.to_owned()))
        .into_evaluated_and_js()
        .0
}

#[cfg(test)]
fn password_requirements(password: &str) -> [bool; 3] {
    let (minimum, mixed_case, number_or_symbol) = password_rules(Expr::from(password.to_owned()));
    [
        minimum.into_evaluated_and_js().0,
        mixed_case.into_evaluated_and_js().0,
        number_or_symbol.into_evaluated_and_js().0,
    ]
}

#[cfg(test)]
mod tests {
    use super::{email_ok, password_requirements, username_ok};

    #[test]
    fn username_matches_main_ascii_pattern_after_ecmascript_trim() {
        for username in ["jane", "A_", "a-", "\u{feff} jane \u{a0}"] {
            assert!(username_ok(username), "{username:?}");
        }
        for username in ["", "a", "Jane Doe", "éa", "a\u{200b}", "a!", "\u{85}ab"] {
            assert!(!username_ok(username), "{username:?}");
        }
    }

    #[test]
    fn email_matches_main_raw_permissive_pattern_without_trimming() {
        for email in ["jane@example.com", "é@例.test", "a@b..c", "a\u{85}@b.c"] {
            assert!(email_ok(email), "{email:?}");
        }
        for email in [
            "",
            "a@b",
            "a@@b.c",
            " a@b.c",
            "a@b.c\n",
            "a@b\u{a0}.c",
            "a@b.c\u{feff}",
            "a@b.c\u{2028}",
        ] {
            assert!(!email_ok(email), "{email:?}");
        }
    }

    #[test]
    fn password_progress_matches_main_utf16_length_and_ascii_regexes() {
        assert_eq!(password_requirements("abcdefgh"), [true, false, false]);
        assert_eq!(password_requirements("Abcdefg_"), [true, true, false]);
        assert_eq!(password_requirements("Abcdefg1"), [true, true, true]);
        assert_eq!(password_requirements("Abcdefgé"), [true, true, true]);
        assert_eq!(password_requirements("😀😀😀😀"), [true, false, true]);
        assert_eq!(password_requirements("Ab1"), [false, true, true]);
        assert_eq!(password_requirements(""), [false, false, false]);
        assert_eq!(password_requirements("________"), [true, false, false]);
        assert_eq!(password_requirements("éééé"), [false, false, true]);
        assert_eq!(password_requirements("😀😀😀a"), [false, false, true]);
        assert_eq!(password_requirements("Äbcdefgh"), [true, false, true]);
    }
}
