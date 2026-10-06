//! Shared Main audit value shortening.

pub(crate) fn short_value(value: Option<&str>, max: usize) -> String {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return "(none)".into();
    };
    // The source collapses runs of LF only, then applies JavaScript's trim.
    static NEWLINES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\n+").expect("pinned activity newline expression is valid")
    });
    let flat = NEWLINES.replace_all(value, " ");
    let flat = super::super::runtime::whitespace::trim_ecmascript(&flat);
    if flat.is_empty() {
        return "(none)".into();
    }
    // String.slice counts UTF-16 code units in the original renderer.
    let units: Vec<_> = flat.encode_utf16().collect();
    if units.len() > max {
        format!("{}…", String::from_utf16_lossy(&units[..max]))
    } else {
        flat.to_owned()
    }
}
