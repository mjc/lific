//! Pure line diff and context folding for expanded Activity values.

pub(super) const MAX_DIFF_LINES: usize = 100_000;
pub(super) const MAX_DIFF_CELLS: usize = 4_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DiffKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiffLine {
    pub(super) kind: DiffKind,
    pub(super) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiffRow {
    Line(DiffLine),
    Fold { count: usize },
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.ends_with('\n') {
        normalized.pop();
    }
    normalized.split('\n').map(str::to_owned).collect()
}

pub(super) fn diff_lines(old: &str, new: &str) -> Option<Vec<DiffLine>> {
    use similar::{
        ChangeTag,
        algorithms::{Capture, lcs},
    };

    let old = split_lines(old);
    let new = split_lines(new);
    if old.len() > MAX_DIFF_LINES || new.len() > MAX_DIFF_LINES {
        return None;
    }
    let as_lines = |lines: &[String], kind| {
        lines
            .iter()
            .map(|text| DiffLine {
                kind,
                text: text.clone(),
            })
            .collect::<Vec<_>>()
    };
    if old.is_empty() {
        return Some(as_lines(&new, DiffKind::Added));
    }
    if new.is_empty() {
        return Some(as_lines(&old, DiffKind::Removed));
    }
    let head = old
        .iter()
        .zip(&new)
        .take_while(|(old, new)| old == new)
        .count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let old_middle = &old[head..old.len() - tail];
    let new_middle = &new[head..new.len() - tail];
    if (old_middle.len() + 1).checked_mul(new_middle.len() + 1)? > MAX_DIFF_CELLS {
        return None;
    }

    // Direct LCS preserves Main's deletion-first tie walk. The convenience
    // capture API also compacts operations, which can move repeated context.
    let mut capture = Capture::new();
    lcs::diff(
        &mut capture,
        old_middle,
        0..old_middle.len(),
        new_middle,
        0..new_middle.len(),
    )
    .expect("Capture has an infallible error type");
    let mut lines = as_lines(&old[..head], DiffKind::Context);
    for op in capture.into_ops() {
        lines.extend(
            op.iter_changes(old_middle, new_middle)
                .map(|change| DiffLine {
                    kind: match change.tag() {
                        ChangeTag::Equal => DiffKind::Context,
                        ChangeTag::Delete => DiffKind::Removed,
                        ChangeTag::Insert => DiffKind::Added,
                    },
                    text: change.value(),
                }),
        );
    }
    lines.extend(as_lines(&old[old.len() - tail..], DiffKind::Context));
    Some(lines)
}

pub(super) fn is_unchanged(lines: &[DiffLine]) -> bool {
    lines.iter().all(|line| line.kind == DiffKind::Context)
}

pub(super) fn fold_context(lines: &[DiffLine]) -> Vec<DiffRow> {
    const CONTEXT_LINES: usize = 3;
    const MIN_FOLD_LINES: usize = 2;
    let mut keep = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if line.kind != DiffKind::Context {
            keep[index.saturating_sub(CONTEXT_LINES)..(index + CONTEXT_LINES + 1).min(lines.len())]
                .fill(true);
        }
    }
    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if keep[index] {
            rows.push(DiffRow::Line(lines[index].clone()));
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && !keep[index] {
            index += 1;
        }
        let count = index - start;
        if count >= MIN_FOLD_LINES {
            rows.push(DiffRow::Fold { count });
        } else {
            rows.extend(lines[start..index].iter().cloned().map(DiffRow::Line));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(kind: DiffKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.into(),
        }
    }

    // Test oracle transcribes Main's trimmed DP walk to check the library's
    // tie behavior over repeated lines without adding a second production engine.
    fn main_oracle(old: &[&str], new: &[&str]) -> Vec<DiffLine> {
        let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let tail = old[head..]
            .iter()
            .rev()
            .zip(new[head..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let a = &old[head..old.len() - tail];
        let b = &new[head..new.len() - tail];
        let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                table[i][j] = if a[i] == b[j] {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        let mut rows = old[..head]
            .iter()
            .map(|text| line(DiffKind::Context, text))
            .collect::<Vec<_>>();
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] == b[j] {
                rows.push(line(DiffKind::Context, a[i]));
                i += 1;
                j += 1;
            } else if table[i + 1][j] >= table[i][j + 1] {
                rows.push(line(DiffKind::Removed, a[i]));
                i += 1;
            } else {
                rows.push(line(DiffKind::Added, b[j]));
                j += 1;
            }
        }
        rows.extend(a[i..].iter().map(|text| line(DiffKind::Removed, text)));
        rows.extend(b[j..].iter().map(|text| line(DiffKind::Added, text)));
        rows.extend(
            old[old.len() - tail..]
                .iter()
                .map(|text| line(DiffKind::Context, text)),
        );
        rows
    }

    #[test]
    fn repeated_small_alphabet_matches_main_trimmed_lcs_oracle() {
        let values = (0..=4)
            .flat_map(|len| {
                (0..1_usize << len).map(move |mask| {
                    (0..len)
                        .map(|index| if mask & (1 << index) == 0 { "a" } else { "b" })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        for old in &values {
            for new in &values {
                assert_eq!(
                    diff_lines(&old.join("\n"), &new.join("\n")),
                    Some(main_oracle(old, new)),
                    "{old:?} -> {new:?}"
                );
            }
        }
    }

    #[test]
    fn replacement_keeps_shared_lines_and_removes_before_adding() {
        assert_eq!(
            diff_lines("head\nold\ntail", "head\nnew\ntail"),
            Some(vec![
                line(DiffKind::Context, "head"),
                line(DiffKind::Removed, "old"),
                line(DiffKind::Added, "new"),
                line(DiffKind::Context, "tail"),
            ])
        );
    }

    #[test]
    fn normalization_preserves_empty_text_and_single_newline_distinction() {
        assert_eq!(diff_lines("", "\n"), Some(vec![line(DiffKind::Added, "")]));
        assert_eq!(
            diff_lines("\n", ""),
            Some(vec![line(DiffKind::Removed, "")])
        );
        assert_eq!(diff_lines("", ""), Some(vec![]));
        assert_eq!(
            diff_lines("a\r\nb\rc\n", "a\nb\nc"),
            Some(vec![
                line(DiffKind::Context, "a"),
                line(DiffKind::Context, "b"),
                line(DiffKind::Context, "c")
            ])
        );
        assert_eq!(
            diff_lines("a\n\n", "a\n"),
            Some(vec![
                line(DiffKind::Context, "a"),
                line(DiffKind::Removed, "")
            ])
        );
    }

    #[test]
    fn lcs_ties_remove_first_without_moving_repeated_context() {
        assert_eq!(
            diff_lines("a\nb", "b\na"),
            Some(vec![
                line(DiffKind::Removed, "a"),
                line(DiffKind::Context, "b"),
                line(DiffKind::Added, "a")
            ])
        );
        assert_eq!(
            diff_lines("a\na\nb", "a\nb\na"),
            Some(vec![
                line(DiffKind::Context, "a"),
                line(DiffKind::Removed, "a"),
                line(DiffKind::Context, "b"),
                line(DiffKind::Added, "a")
            ])
        );
    }

    #[test]
    fn budgets_apply_after_common_ends_trim_but_before_diff_work() {
        let old = (0..5000)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = old.replacen("line2500", "edited", 1);
        let lines = diff_lines(&old, &new).expect("one edit in a long value stays within budget");
        assert_eq!(lines.len(), 5001);
        assert_eq!(lines[2500], line(DiffKind::Removed, "line2500"));
        assert_eq!(lines[2501], line(DiffKind::Added, "edited"));
        assert!(diff_lines(&"old\n".repeat(2000), &"new\n".repeat(2000)).is_none());
        assert!(diff_lines(&"line\n".repeat(MAX_DIFF_LINES + 1), "").is_none());
        assert_eq!(MAX_DIFF_CELLS, 4_000_000);
    }

    #[test]
    fn folds_keep_three_context_rows_near_changes_and_require_two_hidden() {
        let mut lines = (0..8)
            .map(|i| line(DiffKind::Context, &i.to_string()))
            .collect::<Vec<_>>();
        lines.push(line(DiffKind::Added, "edit"));
        lines.extend((8..16).map(|i| line(DiffKind::Context, &i.to_string())));
        let rows = fold_context(&lines);
        assert_eq!(rows.first(), Some(&DiffRow::Fold { count: 5 }));
        assert_eq!(rows.last(), Some(&DiffRow::Fold { count: 5 }));
        assert_eq!(rows.len(), 9);
        let short = [
            line(DiffKind::Context, "0"),
            line(DiffKind::Context, "1"),
            line(DiffKind::Context, "2"),
            line(DiffKind::Context, "3"),
            line(DiffKind::Added, "edit"),
        ];
        assert_eq!(
            fold_context(&short),
            short.iter().cloned().map(DiffRow::Line).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unchanged_values_are_identified_for_whole_value_rendering() {
        assert!(is_unchanged(&[]));
        assert!(is_unchanged(&[line(DiffKind::Context, "same")]));
        assert!(!is_unchanged(&[line(DiffKind::Added, "new")]));
        assert_eq!(
            fold_context(&[line(DiffKind::Context, "a"), line(DiffKind::Context, "b")]),
            vec![DiffRow::Fold { count: 2 }]
        );
    }
}
