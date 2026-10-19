//! Public route validation for native WebSocket admission.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    Redirect(String),
    Issues { project: String },
    Board { project: String },
    IssueDetail { project: String, identifier: String },
    Pages { project: String },
    PageDetail { project: String, page_id: i64 },
}

fn valid_project(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_issue_identifier(value: &str) -> bool {
    let Some((project, sequence)) = value.rsplit_once('-') else {
        return false;
    };
    valid_project(project)
        && !sequence.is_empty()
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

/// Resolve only the public routes supported by the existing web router.
/// Unsupported public paths return `None` and remain not-found routes.
pub(crate) fn resolve(path: &str) -> Option<Route> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let rest = path.strip_prefix("/public/")?;
    let mut parts = rest.split('/');
    let project = parts.next()?;
    if !valid_project(project) {
        return None;
    }
    let project = project.to_owned();
    let remaining = parts.collect::<Vec<_>>();

    match remaining.as_slice() {
        [] | [""] => Some(Route::Redirect(format!("/public/{project}/issues"))),
        [legacy] if valid_issue_identifier(legacy) => Some(Route::Redirect(format!(
            "/public/{project}/issues/{legacy}"
        ))),
        [section] if section.eq_ignore_ascii_case("issues") => Some(Route::Issues { project }),
        [section] if section.eq_ignore_ascii_case("board") => Some(Route::Board { project }),
        [section, identifier]
            if section.eq_ignore_ascii_case("issues") && valid_issue_identifier(identifier) =>
        {
            Some(Route::IssueDetail {
                project,
                identifier: (*identifier).to_owned(),
            })
        }
        [section] if section.eq_ignore_ascii_case("pages") => Some(Route::Pages { project }),
        [section, id]
            if section.eq_ignore_ascii_case("pages")
                && !id.is_empty()
                && id.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Some(Route::PageDetail {
                project,
                page_id: id.parse().ok()?,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_routes_match_the_five_supported_families_and_two_redirects() {
        assert_eq!(
            resolve("/public/ENG"),
            Some(Route::Redirect("/public/ENG/issues".into()))
        );
        assert_eq!(
            resolve("/public/ENG/"),
            Some(Route::Redirect("/public/ENG/issues".into()))
        );
        assert_eq!(
            resolve("/public/ENG/ENG-42?focus=1"),
            Some(Route::Redirect("/public/ENG/issues/ENG-42".into()))
        );
        assert_eq!(
            resolve("/public/ENG/issues"),
            Some(Route::Issues {
                project: "ENG".into()
            })
        );
        assert_eq!(resolve("/public/ENG/ISSUES"), resolve("/public/ENG/issues"));
        assert_eq!(
            resolve("/public/ENG/board"),
            Some(Route::Board {
                project: "ENG".into()
            })
        );
        assert_eq!(resolve("/public/ENG/BOARD"), resolve("/public/ENG/board"));
        assert_eq!(
            resolve("/public/ENG/issues/ENG-42"),
            Some(Route::IssueDetail {
                project: "ENG".into(),
                identifier: "ENG-42".into()
            })
        );
        assert_eq!(
            resolve("/public/ENG/pages"),
            Some(Route::Pages {
                project: "ENG".into()
            })
        );
        assert_eq!(
            resolve("/public/ENG/pages/42"),
            Some(Route::PageDetail {
                project: "ENG".into(),
                page_id: 42
            })
        );
        assert_eq!(
            resolve("/public/ENG/PAGES/42"),
            resolve("/public/ENG/pages/42")
        );
    }

    #[test]
    fn unsupported_public_routes_do_not_fall_through_to_private_routes() {
        for path in [
            "/public/ENG/overview",
            "/public/ENG/settings",
            "/public/ENG/files",
            "/public/ENG/modules",
            "/public/ENG/issues/new",
            "/public/ENG/pages/not-a-number",
            "/ENG/issues/ENG-42",
        ] {
            assert_eq!(resolve(path), None, "{path}");
        }
    }
}
