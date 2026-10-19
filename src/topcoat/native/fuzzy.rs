//! Main's two-tier matcher shared by native page and issue searches.
use super::super::runtime::whitespace::is_ecmascript_whitespace;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Match {
    pub score: f64,
    /// UTF-16 offsets, matching Main's JavaScript string indexing.
    pub start: usize,
    pub end: usize,
}

pub(crate) fn score(query: &str, text: &str) -> Option<f64> {
    find(query, text).map(|matched| matched.score)
}

pub(crate) fn find(query: &str, text: &str) -> Option<Match> {
    if query.is_empty() || text.is_empty() {
        return None;
    }
    let q = query.to_lowercase();
    let t = text.to_lowercase();
    if let Some(start) = t.find(&q) {
        let boundary = start == 0 || t[..start].chars().next_back().is_some_and(is_word_boundary);
        let start = t[..start].encode_utf16().count();
        return Some(Match {
            score: if start == 0 {
                0.95
            } else if boundary {
                0.9
            } else {
                0.8
            },
            start,
            end: start + q.encode_utf16().count(),
        });
    }
    let query_units = q.encode_utf16().collect::<Vec<_>>();
    let text_units = t.encode_utf16().collect::<Vec<_>>();
    let (mut query_index, mut first, mut last) = (0, None, None);
    let (mut current, mut longest, mut boundaries) = (0_usize, 0_usize, 0_usize);
    for (index, unit) in text_units.iter().copied().enumerate() {
        if query_units.get(query_index) != Some(&unit) {
            continue;
        }
        first.get_or_insert(index);
        let consecutive = match last {
            Some(previous) => index == previous + 1,
            None => index == 0,
        };
        if consecutive {
            current += 1;
        } else {
            current = 1;
            if index == 0 || is_word_boundary_unit(text_units[index - 1]) {
                boundaries += 1;
            }
        }
        longest = longest.max(current);
        last = Some(index);
        query_index += 1;
        if query_index == query_units.len() {
            break;
        }
    }
    if query_index != query_units.len() {
        return None;
    }
    let first = first?;
    let last = last?;
    let span = last.saturating_sub(first) + 1;
    let size = query_units.len() as f64;
    Some(Match {
        score: (0.4 * size / span as f64
            + 0.4 * longest as f64 / size
            + 0.2 * boundaries as f64 / size)
            .min(0.7),
        start: first,
        end: last + 1,
    })
}

fn is_word_boundary(character: char) -> bool {
    is_ecmascript_whitespace(character) || "-_/.,()[]{}<>:;!?\"'`".contains(character)
}

fn is_word_boundary_unit(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(is_word_boundary)
}

pub(crate) fn snippet(text: &str, matched: Match) -> String {
    let units = text.encode_utf16().collect::<Vec<_>>();
    let start = matched.start.saturating_sub(40).min(units.len());
    let end = matched.end.saturating_add(40).min(units.len());
    let slice = String::from_utf16_lossy(&units[start..end]);
    let content = slice
        .split(is_ecmascript_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        content,
        if end < units.len() { "…" } else { "" }
    )
}
