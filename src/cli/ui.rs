//! Human-facing terminal output conventions, built on `cliclack` (a Rust
//! implementation of the @clack/prompts look: `┌` intro, `│` gutter, `◇`
//! completed steps, `◆` active prompts, `└` outro).
//!
//! Rules of engagement:
//!
//! - **Only the human path uses this module.** Every command decides
//!   human-vs-JSON via [`crate::cli::term::wants_json`] first; JSON output and
//!   non-TTY fail-fast behavior are untouched. cliclack's underlying `console`
//!   crate additionally strips styling when stdout isn't a terminal, so even a
//!   mis-routed call degrades to plain text rather than ANSI soup.
//! - **Rendering failures are ignored.** These wrappers return `()`, not
//!   `io::Result`: pretty output must never turn a succeeded command into a
//!   failed one because a write to a weird terminal failed.
//! - **One vocabulary.** `step` for completed work, `info` for neutral facts,
//!   `warn`/`error` for problems, `note` for blocks the user must read (keys,
//!   snippets, next steps), `intro`/`outro` bracketing every session.

use std::fmt::Display;

/// Begin a command session: prints the `┌ <title>` header.
pub fn intro(title: &str) {
    let _ = cliclack::intro(
        console::style(format!(" {} ", terminal_line(title)))
            .on_cyan()
            .black()
            .to_string(),
    );
}

/// A completed step: `◇ <msg>`.
pub fn step(msg: impl std::fmt::Display) {
    let _ = cliclack::log::success(terminal_line(msg));
}

/// A neutral informational line: `● <msg>`.
pub fn info(msg: impl std::fmt::Display) {
    let _ = cliclack::log::info(terminal_line(msg));
}

/// A warning line: `▲ <msg>`.
pub fn warn(msg: impl std::fmt::Display) {
    let _ = cliclack::log::warning(terminal_line(msg));
}

/// An error line: `■ <msg>`.
pub fn error(msg: impl std::fmt::Display) {
    let _ = cliclack::log::error(terminal_line(msg));
}

/// A skipped/dimmed line: `◌ <msg>` (rendered via a plain step with dim text).
pub fn skipped(msg: impl std::fmt::Display) {
    let _ = cliclack::log::step(console::style(terminal_line(msg)).dim().to_string());
}

/// A boxed note block with a title — for content the user must actually read
/// (API keys, manual snippets, next steps).
pub fn note(title: impl std::fmt::Display, body: impl std::fmt::Display) {
    let _ = cliclack::note(terminal_line(title), terminal_block(body));
}

/// End the session on a success: `└ <msg>`.
pub fn outro(msg: impl std::fmt::Display) {
    let _ = cliclack::outro(terminal_line(msg));
}

/// End the session on a failure: `└ <msg>` in red.
pub fn outro_cancel(msg: impl std::fmt::Display) {
    let _ = cliclack::outro_cancel(terminal_line(msg));
}

/// Sanitize secondary text (paths, hints) for composition into a UI message.
///
/// Styling is deliberately applied only by the final cliclack call. Returning
/// ANSI from a composable string would make the outer terminal sanitizer
/// display escape bytes literally.
pub fn dim(s: impl std::fmt::Display) -> String {
    terminal_line(s)
}

/// Sanitize a command for composition into a UI message.
pub fn command(s: impl std::fmt::Display) -> String {
    terminal_line(s)
}

pub(crate) fn is_terminal_control(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{00ad}'
                | '\u{0600}'..='\u{0605}'
                | '\u{061c}'
                | '\u{06dd}'
                | '\u{070f}'
                | '\u{0890}'..='\u{0891}'
                | '\u{08e2}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206f}'
                | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{110bd}'
                | '\u{110cd}'
                | '\u{13430}'..='\u{1343f}'
                | '\u{1bca0}'..='\u{1bcaf}'
                | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0001}'
                | '\u{e0020}'..='\u{e007f}'
        )
}

pub(crate) fn terminal_line(value: impl Display) -> String {
    sanitize_terminal_text(&value.to_string(), false)
}

pub(crate) fn terminal_block(value: impl Display) -> String {
    sanitize_terminal_text(&value.to_string(), true)
}

fn sanitize_terminal_text(input: &str, preserve_layout: bool) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\n' | '\t' if preserve_layout => output.push(ch),
            '\x1b' => output.push_str("^["),
            ch if is_terminal_control(ch) => output.push(' '),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{is_terminal_control, terminal_block, terminal_line};
    use proptest::prelude::*;

    #[test]
    fn block_controls_are_neutralized_without_flattening_layout() {
        assert_eq!(
            terminal_block("name\x1b[2J\r\x07\u{85}\u{202e}\u{2066}\nnext\tline\x7f"),
            "name^[[2J     \nnext\tline "
        );
    }

    #[test]
    fn line_controls_cannot_forge_another_status_line() {
        assert_eq!(
            terminal_line("title\n[ok]\tuser\u{061c}\u{200f}\u{2028}\u{206f}"),
            "title [ok] user    "
        );
    }

    proptest! {
        #[test]
        fn line_sanitization_never_emits_terminal_controls(
            input in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(String::from_iter)
        ) {
            let rendered = terminal_line(input);
            prop_assert!(rendered.chars().all(|ch| !is_terminal_control(ch)));
        }

        #[test]
        fn block_sanitization_only_preserves_layout_controls(
            input in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(String::from_iter)
        ) {
            let rendered = terminal_block(input);
            let safe = rendered.chars().all(|ch| {
                !is_terminal_control(ch) || ch == '\n' || ch == '\t'
            });
            prop_assert!(safe);
        }
    }
}
