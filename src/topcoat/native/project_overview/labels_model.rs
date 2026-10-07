//! Pinned label palette, UTF-16 name hash, hex input, and usage.
use crate::db::models::{Issue, Label, Page};
use std::collections::BTreeMap;

pub(super) const PALETTE: [(&str, &str); 12] = [
    ("Red", "#EF4444"),
    ("Orange", "#F97316"),
    ("Amber", "#D97706"),
    ("Green", "#16A34A"),
    ("Emerald", "#059669"),
    ("Teal", "#0D9488"),
    ("Cyan", "#0891B2"),
    ("Blue", "#2563EB"),
    ("Indigo", "#4F46E5"),
    ("Violet", "#7C3AED"),
    ("Pink", "#DB2777"),
    ("Gray", "#6B7280"),
];
pub(super) const PRESETS: [(&str, &str); 6] = [
    ("bug", "#EF4444"),
    ("feature", "#16A34A"),
    ("docs", "#2563EB"),
    ("chore", "#6B7280"),
    ("blocked", "#D97706"),
    ("design", "#7C3AED"),
];
pub(super) const DEFAULT_COLOR: &str = "#6B7280";
pub(crate) fn safe_color(value: &str) -> &str {
    if value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        value
    } else {
        DEFAULT_COLOR
    }
}
pub(super) fn color_name(value: &str) -> &'static str {
    PALETTE
        .iter()
        .find(|(_, color)| color.eq_ignore_ascii_case(value))
        .map_or("Custom", |(name, _)| *name)
}
pub(super) fn color_for_name(value: &str) -> &'static str {
    let hash = value.encode_utf16().fold(0_u32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(u32::from(unit))
    });
    PALETTE[(hash % 11) as usize].1
}
pub(super) fn normalize_hex(value: &str) -> Option<String> {
    let value = value
        .trim_matches(js_whitespace)
        .strip_prefix('#')
        .unwrap_or_else(|| value.trim_matches(js_whitespace));
    if !matches!(value.len(), 3 | 6) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let expanded = if value.len() == 3 {
        value.chars().flat_map(|c| [c, c]).collect()
    } else {
        value.to_owned()
    };
    Some(format!("#{}", expanded.to_ascii_lowercase()))
}
pub(super) use super::super::super::runtime::whitespace::is_ecmascript_whitespace as js_whitespace;

pub(super) fn name_taken(labels: &[Label], value: &str, except: Option<i64>) -> bool {
    let name = value.trim_matches(js_whitespace).to_lowercase();
    labels
        .iter()
        .any(|label| Some(label.id) != except && label.name.to_lowercase() == name)
}
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Usage {
    pub(super) issues: usize,
    pub(super) pages: usize,
}
impl Usage {
    pub(super) fn total(self) -> usize {
        self.issues + self.pages
    }
    pub(super) fn text(self) -> String {
        let mut parts = Vec::new();
        if self.issues > 0 {
            parts.push(format!(
                "{} issue{}",
                self.issues,
                if self.issues == 1 { "" } else { "s" }
            ));
        }
        if self.pages > 0 {
            parts.push(format!(
                "{} page{}",
                self.pages,
                if self.pages == 1 { "" } else { "s" }
            ));
        }
        parts.join(" · ")
    }
}
pub(super) fn usage(issues: &[Issue], pages: &[Page]) -> BTreeMap<String, Usage> {
    let mut result = BTreeMap::<String, Usage>::new();
    for issue in issues {
        for name in &issue.labels {
            result.entry(name.clone()).or_default().issues += 1;
        }
    }
    for page in pages {
        for name in &page.labels {
            result.entry(name.clone()).or_default().pages += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn label_color_hash_uses_js_utf16_wrapping_and_omits_gray() {
        assert_eq!(color_for_name(""), "#EF4444");
        // Golden results from pinned master labelColors.ts, including UTF-16 pairs
        // and the overflowing 32-bit hash. No mirrored implementation here.
        for (name, expected) in [
            ("bug", "#4F46E5"),
            ("feature", "#4F46E5"),
            ("docs", "#F97316"),
            ("chore", "#D97706"),
            ("blocked", "#7C3AED"),
            ("design", "#7C3AED"),
            ("😀", "#2563EB"),
            ("𝄞", "#4F46E5"),
            ("A long enough label name to overflow", "#4F46E5"),
        ] {
            assert_eq!(color_for_name(name), expected);
        }
    }
    #[test]
    fn label_hex_and_safe_legacy_colors_preserve_source_input_rules() {
        assert_eq!(
            normalize_hex(" \u{FEFF}#AbC\u{FEFF} ").as_deref(),
            Some("#aabbcc")
        );
        assert_eq!(normalize_hex("ABC123").as_deref(), Some("#abc123"));
        for input in ["ff", "#ffff", "#1234567", "#12345g", "\u{0085}fff", "red"] {
            assert_eq!(normalize_hex(input), None);
        }
        assert_eq!(safe_color("#abc123"), "#abc123");
        assert_eq!(safe_color("#12aBcF"), "#12aBcF");
        assert_eq!(safe_color("#abc"), DEFAULT_COLOR);
        assert_eq!(color_name("#ef4444"), "Red");
        assert_eq!(color_name("#abcdef"), "Custom");
    }
    #[test]
    fn label_unsafe_stored_color_uses_native_component_fallback() {
        let value = "red; background-image: url(https://example.test)";
        let rendered_color = safe_color(value);
        assert_eq!(rendered_color, "#6B7280");
        assert!(!rendered_color.contains(value));
        assert!(!rendered_color.contains("background-image"));
    }

    #[test]
    fn label_duplicate_test_excludes_rename_target_and_preserves_whitespace() {
        let labels = [Label {
            id: 1,
            project_id: 1,
            name: "BUG".into(),
            color: DEFAULT_COLOR.into(),
        }];
        assert!(name_taken(&labels, " bug ", None));
        assert!(!name_taken(&labels, "bug", Some(1)));
        assert!(!name_taken(&labels, "\u{0085}bug", None));
    }
    #[test]
    fn label_usage_text_counts_issues_and_pages_independently() {
        assert_eq!(
            Usage {
                issues: 1,
                pages: 2
            }
            .text(),
            "1 issue · 2 pages"
        );
        assert_eq!(
            Usage {
                issues: 0,
                pages: 1
            }
            .text(),
            "1 page"
        );
        assert_eq!(Usage::default().text(), "");
    }
    #[test]
    fn label_usage_counts_real_issue_and_page_records_independently() {
        use crate::db::{
            models::{CreateIssue, CreateLabel, CreatePage},
            queries,
        };
        let (db, _, _, _, _, _, project) = crate::api::test_helpers::setup_membership_test();
        let conn = db.write().unwrap();
        for name in ["bug", "docs"] {
            queries::create_label(
                &conn,
                &CreateLabel {
                    project_id: project,
                    name: name.into(),
                    color: DEFAULT_COLOR.into(),
                },
            )
            .unwrap();
        }
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: project,
                title: "Usage".into(),
                labels: vec!["bug".into(), "docs".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let pages = ["bug", "docs"]
            .into_iter()
            .map(|name| {
                queries::create_page(
                    &conn,
                    &CreatePage {
                        project_id: Some(project),
                        title: name.into(),
                        labels: vec![name.into()],
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let counts = usage(&[issue], &pages);
        assert_eq!(counts["bug"].issues, 1);
        assert_eq!(counts["bug"].pages, 1);
        assert_eq!(counts["docs"].issues, 1);
        assert_eq!(counts["docs"].pages, 1);
        assert_eq!(counts["bug"].total(), 2);
        assert_eq!(counts["docs"].text(), "1 issue · 1 page");
    }
}
