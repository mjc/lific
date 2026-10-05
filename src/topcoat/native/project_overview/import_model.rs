// Exactly the master's trimmed /^[^/\s]+\/[^/\s]+$/ decision, executed in Rust.
pub(super) fn valid_repository(repo: &str) -> bool {
    let repo = repo.trim_matches(super::labels_model::js_whitespace);
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    !owner.is_empty()
        && !name.is_empty()
        && parts.next().is_none()
        && !owner
            .chars()
            .chain(name.chars())
            .any(super::labels_model::js_whitespace)
}

#[cfg(test)]
mod tests {
    #[test]
    fn repository_shape_keeps_master_ecmascript_whitespace_semantics() {
        for repo in [
            "owner/name",
            "  owner/name  ",
            "\u{feff}owner/name\u{feff}",
            "\u{0085}owner/name",
        ] {
            assert!(super::valid_repository(repo), "{repo:?}");
        }
        for repo in [
            "",
            "/name",
            "owner/",
            "owner/name/extra",
            "owner name/repo",
            "owner/re po",
            "owner\u{00a0}/repo",
            "owner/\u{feff}repo",
        ] {
            assert!(!super::valid_repository(repo), "{repo:?}");
        }
    }
}
