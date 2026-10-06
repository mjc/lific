//! Display labels and initials shared by native user and activity views.

pub(crate) fn display_name<'a>(
    display: Option<&'a str>,
    username: Option<&'a str>,
    fallback: &'a str,
) -> &'a str {
    display
        .filter(|name| !name.is_empty())
        .or_else(|| username.filter(|name| !name.is_empty()))
        .unwrap_or(fallback)
}

pub(crate) fn initials(label: &str) -> String {
    // Main takes the first UTF-16 unit of its first two split segments.
    let separator = |character: char| {
        character == '_'
            || character == '-'
            || super::super::runtime::whitespace::is_ecmascript_whitespace(character)
    };
    let count = if label.chars().next().is_some_and(separator) {
        1
    } else {
        2
    };
    label
        .split(separator)
        .filter(|word| !word.is_empty())
        .take(count)
        .filter_map(|word| word.encode_utf16().next())
        .map(|unit| char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER))
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_collapse_repeated_separators_like_the_original_user_select() {
        assert_eq!(initials("Mary  Jane"), "MJ");
        assert_eq!(initials("foo__bar"), "FB");
        assert_eq!(initials("one_- two-three"), "OT");
        assert_eq!(initials("  Mary Jane"), "M");
        assert_eq!(initials("\u{feff}Mary Jane"), "M");
        assert_eq!(initials("Mary\u{0085}Jane Smith"), "MS");
    }

    #[test]
    fn initials_use_main_utf16_first_units_for_supplementary_characters() {
        assert_eq!(initials("🦀 Crab"), "\u{fffd}C");
        assert_eq!(initials("🦀 🐈"), "\u{fffd}\u{fffd}");
    }
}
