//! Pure editing helpers shared by future native Markdown composers.
//!
//! Browser selection offsets and the returned caret use UTF-16 code units.
//! Nonfinite, reversed and out-of-range offsets are normalized at this helper's
//! input boundary, following the pinned original insertSnippetAt implementation.
//! JavaScript can split a surrogate pair; valid Rust strings cannot represent
//! that result. The split-surrogate contract must be explicit, never lossy.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Insertion {
    pub(crate) text: String,
    pub(crate) caret: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SelectionError {
    SplitSurrogatePair,
}

pub(crate) fn markdown_for(filename: &str, mime: &str, url: &str) -> String {
    let marker = if mime.starts_with("image/") { "!" } else { "" };
    format!("{marker}[{filename}]({url})")
}

pub(crate) fn insert_snippet_at(
    current: &str,
    selection_start: f64,
    selection_end: f64,
    snippet: &str,
) -> Result<Insertion, SelectionError> {
    let (start, end) = normalized_selection(current, selection_start, selection_end)?;
    let before = &current[..start];
    let after = &current[end..];
    let leading_break = if !before.is_empty() && !before.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let insertion = format!("{leading_break}{snippet}\n");
    Ok(Insertion {
        caret: before.encode_utf16().count() + insertion.encode_utf16().count(),
        text: format!("{before}{insertion}{after}"),
    })
}

/// Normalize original JavaScript offsets and validate UTF-16 boundaries once.
#[allow(
    clippy::cast_sign_loss,
    reason = "offsets are clamped nonnegative before JavaScript slice truncation"
)]
fn normalized_selection(
    current: &str,
    selection_start: f64,
    selection_end: f64,
) -> Result<(usize, usize), SelectionError> {
    let length = current.encode_utf16().count();
    let raw_start = if selection_start.is_finite() {
        selection_start
    } else {
        length as f64
    };
    let raw_end = if selection_end.is_finite() {
        selection_end
    } else {
        raw_start
    };
    let start = raw_start.min(raw_end).max(0.0).min(length as f64);
    let end = raw_start.max(raw_end).max(start).min(length as f64);
    // String.slice converts fractional finite offsets to integers toward zero.
    let start = start.trunc() as usize;
    let end = end.trunc() as usize;
    let mut start_byte = (start == length).then_some(current.len());
    let mut end_byte = (end == length).then_some(current.len());
    let mut offset = 0;
    for (byte, character) in current.char_indices() {
        if offset == start {
            start_byte = Some(byte);
        }
        if offset == end {
            end_byte = Some(byte);
        }
        offset += character.len_utf16();
    }
    match (start_byte, end_byte) {
        (Some(start), Some(end)) => Ok((start, end)),
        _ => Err(SelectionError::SplitSurrogatePair),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNIPPET: &str = "![shot.png](/api/attachments/7)";

    // These nine tests port pinned master web/tests/insertSnippet.test.ts.
    #[test]
    fn native_markdown_edit_images_embed_everything_else_links() {
        assert_eq!(
            markdown_for("shot.png", "image/png", "/api/attachments/7"),
            SNIPPET
        );
        assert_eq!(
            markdown_for("trace.log", "text/plain", "/api/attachments/7"),
            "[trace.log](/api/attachments/7)"
        );
    }

    #[test]
    fn native_markdown_edit_empty_composer_has_no_leading_break() {
        let insertion = insert_snippet_at("", 0.0, 0.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("{SNIPPET}\n"));
        assert_eq!(insertion.caret, SNIPPET.len() + 1);
    }

    #[test]
    fn native_markdown_edit_midline_insertion_starts_its_own_block() {
        let insertion = insert_snippet_at("see this", 8.0, 8.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("see this\n{SNIPPET}\n"));
        assert_eq!(insertion.caret, insertion.text.len());
    }

    #[test]
    fn native_markdown_edit_fresh_line_insertion_has_no_extra_leading_break() {
        let insertion = insert_snippet_at("intro\n", 6.0, 6.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("intro\n{SNIPPET}\n"));
        assert_eq!(insertion.caret, insertion.text.len());
    }

    #[test]
    fn native_markdown_edit_middle_insertion_keeps_tail_and_caret_before_it() {
        let insertion = insert_snippet_at("before\nafter", 7.0, 7.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("before\n{SNIPPET}\nafter"));
        assert_eq!(insertion.caret, format!("before\n{SNIPPET}\n").len());
        assert_eq!(&insertion.text[insertion.caret..], "after");
    }

    #[test]
    fn native_markdown_edit_selection_is_replaced() {
        let insertion = insert_snippet_at("keep DROP keep", 5.0, 9.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("keep \n{SNIPPET}\n keep"));
        assert_eq!(&insertion.text[insertion.caret..], " keep");
    }

    #[test]
    fn native_markdown_edit_backwards_selection_is_normalized() {
        let forward = insert_snippet_at("keep DROP keep", 5.0, 9.0, SNIPPET).unwrap();
        let backward = insert_snippet_at("keep DROP keep", 9.0, 5.0, SNIPPET).unwrap();
        assert_eq!(backward, forward);
    }

    #[test]
    fn native_markdown_edit_out_of_range_offsets_clamp_to_text() {
        let insertion = insert_snippet_at("short", 999.0, 999.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("short\n{SNIPPET}\n"));
        assert_eq!(insertion.caret, insertion.text.len());
        let negative = insert_snippet_at("short", -4.0, -4.0, SNIPPET).unwrap();
        assert_eq!(negative.text, format!("{SNIPPET}\nshort"));
        assert_eq!(negative.caret, SNIPPET.len() + 1);
    }

    #[test]
    fn native_markdown_edit_consecutive_inserts_stack_one_per_line() {
        let first = insert_snippet_at("", 0.0, 0.0, "![a](/api/attachments/1)").unwrap();
        let caret = f64::from(u32::try_from(first.caret).unwrap());
        let second =
            insert_snippet_at(&first.text, caret, caret, "![b](/api/attachments/2)").unwrap();
        assert_eq!(
            second.text,
            "![a](/api/attachments/1)\n![b](/api/attachments/2)\n"
        );
        assert_eq!(second.caret, second.text.len());
    }

    // Additional characterization of JavaScript String/selection semantics.
    // These cases were not present in the original nine-test source.
    #[test]
    fn native_markdown_edit_characterization_uses_utf16_selection_and_caret_offsets() {
        let snippet = "![📷](/api/attachments/7)";
        let insertion = insert_snippet_at("A😀e\u{301}尾Z", 3.0, 5.0, snippet).unwrap();
        assert_eq!(insertion.text, format!("A😀\n{snippet}\n尾Z"));
        assert_eq!(insertion.caret, 3 + 1 + snippet.encode_utf16().count() + 1);
    }

    #[test]
    fn native_markdown_edit_characterization_replaces_a_complete_surrogate_pair() {
        let insertion = insert_snippet_at("😀tail", 0.0, 2.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("{SNIPPET}\ntail"));
        assert_eq!(insertion.caret, SNIPPET.encode_utf16().count() + 1);
    }

    #[test]
    fn native_markdown_edit_characterization_nonfinite_start_defaults_to_utf16_end() {
        for start in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let insertion = insert_snippet_at("😀", start, f64::NAN, SNIPPET).unwrap();
            assert_eq!(insertion.text, format!("😀\n{SNIPPET}\n"));
            assert_eq!(insertion.caret, 2 + 1 + SNIPPET.encode_utf16().count() + 1);
        }
    }

    #[test]
    fn native_markdown_edit_characterization_nonfinite_end_defaults_to_raw_start() {
        let insertion = insert_snippet_at("short", 0.0, f64::INFINITY, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("{SNIPPET}\nshort"));
        assert_eq!(insertion.caret, SNIPPET.len() + 1);
    }

    #[test]
    fn native_markdown_edit_characterization_nonfinite_start_then_normalizes_finite_end() {
        let insertion = insert_snippet_at("short", f64::NAN, 1.0, SNIPPET).unwrap();
        assert_eq!(insertion.text, format!("s\n{SNIPPET}\n"));
        assert_eq!(insertion.caret, insertion.text.len());
    }

    // Native representation constraint, not inherited JavaScript behavior:
    // Rust String cannot retain an unpaired UTF-16 surrogate after a splice.
    #[test]
    fn native_markdown_edit_representation_constraint_rejects_split_surrogate_pairs() {
        for (start, end) in [(2.0, 2.0), (1.0, 2.0), (2.0, 3.0)] {
            assert_eq!(
                insert_snippet_at("A😀Z", start, end, SNIPPET),
                Err(SelectionError::SplitSurrogatePair)
            );
        }
    }
}
