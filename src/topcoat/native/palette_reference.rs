// Initial native issue-reference slice. These cases come from pinned master
// 9683d38, web/src/lib/paletteSearch.ts and web/tests/paletteSearch.test.ts.
// Page references are classified so the issue resolver cannot consume them;
// page resolution and the rest of the global palette remain separate work.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReferenceKind {
    Issue,
    Page,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub(crate) kind: ReferenceKind,
    pub(crate) project: Option<String>,
    pub(crate) number: i64,
}

pub(crate) fn parse_reference(query: &str) -> Option<Reference> {
    let query = query.trim_matches(js_whitespace);
    let prefix = query.trim_end_matches(|ch: char| ch.is_ascii_digit());
    let digits = &query[prefix.len()..];
    if digits.is_empty() {
        return None;
    }
    let number = digits.parse().ok()?;
    if prefix.is_empty() || prefix == "#" {
        return Some(Reference {
            kind: ReferenceKind::Issue,
            project: None,
            number,
        });
    }
    let stem = prefix.trim_end_matches(|ch| ch == '-' || js_whitespace(ch));
    if stem.eq_ignore_ascii_case("doc") {
        return Some(Reference {
            kind: ReferenceKind::Page,
            project: None,
            number,
        });
    }
    let doc_start = stem.len().saturating_sub(3);
    if stem
        .get(doc_start..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case("doc"))
    {
        let project = stem[..doc_start].trim_end_matches(|ch| ch == '-' || js_whitespace(ch));
        if valid_project(project) {
            return Some(Reference {
                kind: ReferenceKind::Page,
                project: Some(project.to_owned()),
                number,
            });
        }
    }
    valid_project(stem).then(|| Reference {
        kind: ReferenceKind::Issue,
        project: Some(stem.to_owned()),
        number,
    })
}

fn valid_project(project: &str) -> bool {
    let mut chars = project.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

// Match JavaScript trim/\s, rather than Rust's different Unicode whitespace set.
fn js_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IssueHit {
    pub(crate) title: String,
    pub(crate) identifier: String,
    pub(crate) status: String,
    pub(crate) project_name: String,
    pub(crate) logical_destination: String,
}

pub(crate) fn issue_hits(
    db: &crate::db::DbPool,
    identity: &Option<crate::resolve_caller::ResolvedIdentity>,
    query: &str,
    current_project: Option<&str>,
) -> Result<Vec<IssueHit>, crate::error::LificError> {
    crate::api::require_user(identity)?;
    let Some(reference) = parse_reference(query) else {
        return Ok(Vec::new());
    };
    if reference.kind != ReferenceKind::Issue {
        return Ok(Vec::new());
    }
    let mut projects = crate::services::projects::list_visible_projects(db, identity)?;
    if let Some(project) = reference.project {
        projects.retain(|candidate| candidate.identifier.eq_ignore_ascii_case(&project));
    } else if let Some(current) = current_project {
        // Stable ordering retains the personal catalog order for other projects.
        projects.sort_by_key(|project| !project.identifier.eq_ignore_ascii_case(current));
    }
    let mut hits = Vec::new();
    for project in projects {
        let identifier = format!("{}-{}", project.identifier, reference.number);
        let issue = match crate::services::issues::resolve_issue(db, identity, &identifier) {
            Ok(issue) => issue,
            Err(crate::error::LificError::NotFound(_) | crate::error::LificError::Forbidden(_)) => {
                continue;
            }
            Err(error) => return Err(error),
        };
        hits.push(IssueHit {
            logical_destination: format!("/{}/issues/{}", project.identifier, issue.identifier),
            title: issue.title,
            identifier: issue.identifier,
            status: issue.status.to_string(),
            project_name: project.name,
        });
        if hits.len() == 8 {
            break;
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::{Reference, ReferenceKind, issue_hits, parse_reference};
    use crate::{
        actor::Transport,
        db::{
            DbPool,
            models::{CreateIssue, CreateProject, Role, Status},
            queries,
        },
        error::LificError,
        resolve_caller::ResolvedIdentity,
    };

    #[test]
    fn native_palette_reference_bare_or_hashed_number_leaves_project_to_caller() {
        for query in ["34", " #34 "] {
            assert_eq!(
                parse_reference(query),
                Some(Reference {
                    kind: ReferenceKind::Issue,
                    project: None,
                    number: 34,
                }),
                "query: {query:?}",
            );
        }
    }

    #[test]
    fn native_palette_reference_qualified_issue_accepts_original_spellings() {
        for (query, project) in [
            ("FIC34", "FIC"),
            ("fic 34", "fic"),
            ("FIC-34", "FIC"),
            ("fic-034", "fic"),
        ] {
            assert_eq!(
                parse_reference(query),
                Some(Reference {
                    kind: ReferenceKind::Issue,
                    project: Some(project.to_owned()),
                    number: 34,
                }),
                "query: {query:?}",
            );
        }
    }

    #[test]
    fn native_palette_reference_unqualified_doc_is_page_before_issue_matching() {
        for query in ["doc 3", "DOC-3"] {
            assert_eq!(
                parse_reference(query),
                Some(Reference {
                    kind: ReferenceKind::Page,
                    project: None,
                    number: 3,
                }),
                "query: {query:?}",
            );
        }
    }

    #[test]
    fn native_palette_reference_qualified_doc_is_page_before_issue_matching() {
        for (query, project) in [
            ("lif doc 3", "lif"),
            ("LIF-DOC-3", "LIF"),
            ("lifdoc3", "lif"),
        ] {
            assert_eq!(
                parse_reference(query),
                Some(Reference {
                    kind: ReferenceKind::Page,
                    project: Some(project.to_owned()),
                    number: 3,
                }),
                "query: {query:?}",
            );
        }
    }

    #[test]
    fn native_palette_reference_ordinary_text_is_not_a_reference() {
        for query in ["", "status", "fix 34 bugs", "34 bugs", "#", "3.4"] {
            assert_eq!(parse_reference(query), None, "query: {query:?}");
        }
    }

    #[test]
    fn native_palette_reference_characterizes_original_repeated_separators() {
        // Additional characterization from the original [\s-]* regex;
        // these examples are not cases inherited from paletteSearch.test.ts.
        for query in ["FIC--34", "FIC - 34"] {
            assert_eq!(
                parse_reference(query),
                Some(Reference {
                    kind: ReferenceKind::Issue,
                    project: Some("FIC".to_owned()),
                    number: 34,
                }),
            );
        }
        assert_eq!(
            parse_reference("LIF - DOC--3"),
            Some(Reference {
                kind: ReferenceKind::Page,
                project: Some("LIF".to_owned()),
                number: 3,
            }),
        );
    }

    fn fixture() -> (DbPool, ResolvedIdentity, i64, i64) {
        let (db, _, _, _, viewer, _, first_id) = crate::api::test_helpers::setup_membership_test();
        let second_id = {
            let conn = db.write().unwrap();
            let second = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "SEC".into(),
                    name: "Second visible project".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let hidden = queries::create_project(
                &conn,
                &CreateProject {
                    identifier: "HIDE".into(),
                    name: "Private hidden project".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            queries::members::upsert_member(&conn, second.id, viewer.id, Role::Viewer).unwrap();
            let visible = Some([first_id, second.id].into_iter().collect());
            queries::reorder_projects(&conn, viewer.id, &[second.id, first_id], &visible).unwrap();
            for (project_id, title) in [
                (first_id, "Visible <img src=x> \"title\""),
                (second.id, "Second visible issue"),
                (hidden.id, "Private hidden issue"),
            ] {
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id,
                        title: title.into(),
                        status: Status::Active,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            second.id
        };
        let identity = ResolvedIdentity {
            user: crate::auth::fresh_auth_user(&viewer),
            transport: Transport::Web,
        };
        (db, identity, first_id, second_id)
    }

    #[test]
    fn native_palette_reference_reads_qualified_hits_as_plain_authorized_projection() {
        let (db, identity, _, _) = fixture();
        for query in ["MEM1", "mem 1", "MEM-1", "mem-001"] {
            let hits = issue_hits(&db, &Some(identity.clone()), query, None).unwrap();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].title, "Visible <img src=x> \"title\"");
            assert_eq!(hits[0].identifier, "MEM-1");
            assert_eq!(hits[0].status, "active");
            assert_eq!(hits[0].project_name, "Membership Test");
            assert_eq!(hits[0].logical_destination, "/MEM/issues/MEM-1");
        }
    }

    #[test]
    fn native_palette_reference_reads_bare_numbers_in_personal_order_with_current_first() {
        let (db, identity, _, _) = fixture();
        for query in ["1", " #1 "] {
            let hits = issue_hits(&db, &Some(identity.clone()), query, None).unwrap();
            assert_eq!(
                hits.iter()
                    .map(|hit| hit.identifier.as_str())
                    .collect::<Vec<_>>(),
                ["SEC-1", "MEM-1"]
            );
            let current = issue_hits(&db, &Some(identity.clone()), query, Some("mem")).unwrap();
            assert_eq!(
                current
                    .iter()
                    .map(|hit| hit.identifier.as_str())
                    .collect::<Vec<_>>(),
                ["MEM-1", "SEC-1"]
            );
        }
    }

    #[test]
    fn native_palette_reference_reads_omit_hidden_missing_and_page_candidates() {
        let (db, identity, first_id, _) = fixture();
        for query in [
            "HIDE-1",
            "UNKNOWN-1",
            "MEM-99",
            "MEM-DOC-1",
            "doc 1",
            "lifdoc3",
        ] {
            assert!(
                issue_hits(&db, &Some(identity.clone()), query, None)
                    .unwrap()
                    .is_empty()
            );
        }
        {
            let conn = db.write().unwrap();
            conn.execute(
                "DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2",
                [first_id, identity.user.id],
            )
            .unwrap();
        }
        assert!(
            issue_hits(&db, &Some(identity.clone()), "MEM-1", None)
                .unwrap()
                .is_empty()
        );
        let remaining = issue_hits(&db, &Some(identity), "1", Some("MEM")).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].identifier, "SEC-1");
    }

    #[test]
    fn native_palette_reference_reads_cap_issue_group_at_original_eight() {
        let (db, identity, _, _) = fixture();
        {
            let conn = db.write().unwrap();
            for index in 0..9 {
                let project = queries::create_project(
                    &conn,
                    &CreateProject {
                        identifier: format!("P{index}"),
                        name: format!("Project {index}"),
                        ..Default::default()
                    },
                )
                .unwrap();
                queries::members::upsert_member(&conn, project.id, identity.user.id, Role::Viewer)
                    .unwrap();
                queries::create_issue(
                    &conn,
                    &CreateIssue {
                        project_id: project.id,
                        title: format!("Issue {index}"),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        let hits = issue_hits(&db, &Some(identity), "1", None).unwrap();
        assert_eq!(hits.len(), 8);
        assert!(hits.iter().all(|hit| !hit.identifier.starts_with("HIDE-")));
        assert_eq!(
            hits.iter()
                .map(|hit| &hit.identifier)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            8
        );
    }

    #[test]
    fn native_palette_reference_reads_require_identity_and_preserve_database_faults() {
        let (db, identity, _, _) = fixture();
        assert!(matches!(
            issue_hits(&db, &None, "MEM-1", None),
            Err(LificError::Forbidden(_))
        ));
        db.write()
            .unwrap()
            .execute("ALTER TABLE issues RENAME TO unavailable_issues", [])
            .unwrap();
        assert!(matches!(
            issue_hits(&db, &Some(identity), "MEM-1", None),
            Err(LificError::Database(_))
        ));
    }
}
