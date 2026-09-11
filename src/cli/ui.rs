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

use std::fmt::{self, Display};

/// Begin a command session: prints the `┌ <title>` header.
pub fn intro(title: &str) {
    let _ = cliclack::intro(
        console::style(format_args!(" {} ", title.terminal_line()))
            .on_cyan()
            .black()
            .to_string(),
    );
}

/// A completed step: `◇ <msg>`.
pub fn step(msg: impl std::fmt::Display) {
    let _ = cliclack::log::success(msg.terminal_line());
}

/// A neutral informational line: `● <msg>`.
pub fn info(msg: impl std::fmt::Display) {
    let _ = cliclack::log::info(msg.terminal_line());
}

/// A warning line: `▲ <msg>`.
pub fn warn(msg: impl std::fmt::Display) {
    let _ = cliclack::log::warning(msg.terminal_line());
}

/// An error line: `■ <msg>`.
pub fn error(msg: impl std::fmt::Display) {
    let _ = cliclack::log::error(msg.terminal_line());
}

/// A skipped/dimmed line: `◌ <msg>` (rendered via a plain step with dim text).
pub fn skipped(msg: impl std::fmt::Display) {
    let _ = cliclack::log::step(console::style(msg.terminal_line()).dim().to_string());
}

/// A boxed note block with a sanitized title.
///
/// Callers choose the body policy: prose and credentials use
/// [`terminal_block`], while format-aware configuration previews remain
/// lossless until their dedicated presentation slice handles them.
pub fn note(title: impl std::fmt::Display, body: impl std::fmt::Display) {
    // The body policy is selected by the caller. Configuration snippets stay
    // byte-faithful until the format-aware preview slice handles them.
    let _ = cliclack::note(title.terminal_line(), body);
}

/// End the session on a success: `└ <msg>`.
pub fn outro(msg: impl std::fmt::Display) {
    let _ = cliclack::outro(msg.terminal_line());
}

/// End the session on a failure: `└ <msg>` in red.
pub fn outro_cancel(msg: impl std::fmt::Display) {
    let _ = cliclack::outro_cancel(msg.terminal_line());
}

/// Sanitize secondary text (paths, hints) for composition into a UI message.
///
/// Styling is deliberately applied only by the final cliclack call. Returning
/// ANSI from a composable string would make the outer terminal sanitizer
/// display escape bytes literally.
pub fn dim(s: impl std::fmt::Display) -> impl Display {
    s.terminal_line()
}

/// Sanitize a command for composition into a UI message.
pub fn command(s: impl std::fmt::Display) -> impl Display {
    s.terminal_line()
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

pub(crate) fn terminal_line(value: impl Display) -> impl Display {
    value.terminal_line()
}

pub(crate) fn terminal_block(value: impl Display) -> impl Display {
    value.terminal_block()
}

pub(crate) trait TerminalDisplay: Display + Sized {
    fn terminal_line(self) -> Terminal<Self> {
        Terminal {
            value: self,
            preserve_layout: false,
        }
    }

    fn terminal_block(self) -> Terminal<Self> {
        Terminal {
            value: self,
            preserve_layout: true,
        }
    }
}

impl<T: Display> TerminalDisplay for T {}

pub(crate) struct Terminal<T> {
    value: T,
    preserve_layout: bool,
}

impl<T: Display> Display for Terminal<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut writer = TerminalWriter {
            formatter,
            preserve_layout: self.preserve_layout,
        };
        if writer.formatter.alternate() {
            fmt::write(&mut writer, format_args!("{:#}", &self.value))
        } else {
            fmt::write(&mut writer, format_args!("{}", &self.value))
        }
    }
}

struct TerminalWriter<'a, 'b> {
    formatter: &'a mut fmt::Formatter<'b>,
    preserve_layout: bool,
}

impl fmt::Write for TerminalWriter<'_, '_> {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let mut safe_start = 0;
        for (index, ch) in input.char_indices() {
            let replacement = match ch {
                '\n' | '\t' if self.preserve_layout => None,
                '\x1b' => Some("^["),
                ch if is_terminal_control(ch) => Some(" "),
                _ => None,
            };
            let Some(replacement) = replacement else {
                continue;
            };
            self.formatter.write_str(&input[safe_start..index])?;
            self.formatter.write_str(replacement)?;
            safe_start = index + ch.len_utf8();
        }
        self.formatter.write_str(&input[safe_start..])
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{self, Write as _};

    use super::{TerminalDisplay, is_terminal_control, terminal_block, terminal_line};
    use proptest::prelude::*;

    #[test]
    fn block_controls_are_neutralized_without_flattening_layout() {
        assert_eq!(
            terminal_block("name\x1b[2J\r\x07\u{85}\u{202e}\u{2066}\nnext\tline\x7f").to_string(),
            "name^[[2J     \nnext\tline "
        );
    }

    #[test]
    fn line_controls_cannot_forge_another_status_line() {
        assert_eq!(
            terminal_line("title\n[ok]\tuser\u{061c}\u{200f}\u{2028}\u{206f}").to_string(),
            "title [ok] user    "
        );
    }

    #[test]
    fn sanitization_preserves_safe_input_and_is_idempotent() {
        let safe = "ordinary text\nwith\ttabs";
        assert_eq!(terminal_block(safe).to_string(), safe);

        let once = terminal_block("bad\x1b[2J\n\ttext").to_string();
        assert_eq!(terminal_block(&once).to_string(), once);
    }

    struct Fragments;

    impl fmt::Display for Fragments {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("prefix")?;
            formatter.write_str("\t")?;
            formatter.write_str("suffix")
        }
    }

    #[test]
    fn adapter_handles_fragmented_formatting() {
        let mut output = String::new();
        write!(&mut output, "{}", Fragments.terminal_block()).unwrap();
        assert_eq!(output, "prefix\tsuffix");
    }

    struct FailingWriter;

    impl fmt::Write for FailingWriter {
        fn write_str(&mut self, _input: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }

    #[test]
    fn adapter_propagates_destination_errors() {
        let mut output = FailingWriter;
        let error = write!(&mut output, "{}", "text".terminal_line());
        assert!(error.is_err());
    }

    proptest! {
        #[test]
        fn line_sanitization_never_emits_terminal_controls(
            input in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(String::from_iter)
        ) {
            let rendered = terminal_line(input).to_string();
            prop_assert!(rendered.chars().all(|ch| !is_terminal_control(ch)));
        }

        #[test]
        fn block_sanitization_only_preserves_layout_controls(
            input in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(String::from_iter)
        ) {
            let rendered = terminal_block(input).to_string();
            let safe = rendered.chars().all(|ch| {
                !is_terminal_control(ch) || ch == '\n' || ch == '\t'
            });
            prop_assert!(safe);
        }
    }
}
