//! Validation of proxy mount prefixes used by native transport.

/// A proxy prefix is a path made from ordinary URL segments, never a URL.
/// Returning the normalized borrowed path keeps browser and response URLs equal.
pub(crate) fn forwarded_prefix(value: &str) -> Option<&str> {
    let prefix = value.trim_end_matches('/');
    (prefix.starts_with('/')
        && !prefix.is_empty()
        && prefix[1..].split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
                })
        }))
    .then_some(prefix)
}

#[cfg(test)]
mod tests {
    use super::forwarded_prefix;

    #[test]
    fn forwarded_prefix_accepts_paths_and_refuses_url_or_traversal_inputs() {
        assert_eq!(forwarded_prefix("/app/"), Some("/app"));
        assert_eq!(forwarded_prefix("/team/lific"), Some("/team/lific"));
        for invalid in [
            "",
            "/",
            "app",
            "//evil.test",
            "/app//nested",
            "/app/../other",
            "/app/./other",
            "/app?token=1",
            "/app#fragment",
            "/app%2fother",
            "/app\\other",
            "/app, /other",
            "/app\"",
            "https://evil.test",
        ] {
            assert_eq!(forwarded_prefix(invalid), None, "{invalid}");
        }
    }
}
