//! Authenticate the native multipart boundary before consuming its body.

use axum::{
    extract::{DefaultBodyLimit, FromRequest, Multipart},
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use topcoat::{
    context::{Cx, app_context},
    router::{Body, path_param, request, response::Response, route},
    runtime::Surrogated,
};

use super::super::context;
use super::model::{ImportReport, ImportResult, ImportedProject, RowCount};
use crate::error::LificError;
use crate::services::project_archive_import;

path_param!(account);

#[route(POST "/__native_project_import/upload/{account}")]
async fn upload(cx: &Cx, body: Body) -> topcoat::Result<Response> {
    let gate = (|| {
        let expected = path_param::<Account>(cx)
            .parse::<i64>()
            .map_err(|_| LificError::BadRequest("invalid account".into()))?;
        let caller = context::caller(cx)?;
        let headers = caller.session_headers()?;
        let session = crate::services::project_archive_export::require_human_session(
            context::db(cx),
            &caller.identity,
            &headers,
        )?;
        if session.user_id != expected || !session.is_admin {
            return Err(LificError::Forbidden(
                "only the signed-in admin can import a project archive".into(),
            ));
        }
        let expected_session = request::headers(cx)
            .get("x-lific-import-session")
            .and_then(|header| header.to_str().ok());
        let fingerprint = super::state::session_fingerprint(Some(&session.token));
        if expected_session != Some(fingerprint.as_str()) {
            return Err(LificError::Forbidden(
                "Your session changed. Reload this page.".into(),
            ));
        }
        Ok((caller, headers))
    })();
    let response = match gate {
        Ok((caller, headers)) => {
            let mut request = axum::http::Request::from_parts(
                request::parts(cx).clone(),
                axum::body::Body::new(body),
            );
            DefaultBodyLimit::max(project_archive_import::ARCHIVE_UPLOAD_BODY_LIMIT)
                .apply(&mut request);
            match Multipart::from_request(request, &()).await {
                Ok(multipart) => {
                    let imported = project_archive_import::upload(
                        context::db(cx).clone(),
                        app_context::<crate::storage::AttachmentStore>(cx).clone(),
                        app_context::<crate::realtime::RealtimeHub>(cx).clone(),
                        &caller.identity,
                        &headers,
                        multipart,
                        crate::project_archive::Limits::WEB,
                    )
                    .await;
                    match imported {
                        Ok(imported) => (
                            StatusCode::CREATED,
                            axum::Json(report(imported).into_surrogate()),
                        )
                            .into_response()
                            .map(Body::new),
                        Err(error) => failure(error),
                    }
                }
                Err(error) => error.into_response().map(Body::new),
            }
        }
        Err(error) => failure(error),
    };
    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(Response::from_parts(parts, body))
}

fn failure(error: LificError) -> Response {
    let message = error.client_message().to_owned();
    let (mut parts, _) = error.into_response().into_parts();
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    Response::from_parts(parts, Body::from(message))
}

fn report(imported: project_archive_import::ImportResponse) -> ImportResult {
    // This display copy travels through a reactive shard with a 2 MiB body
    // limit. Even JSON's worst-case escaping stays below it. Full reference
    // messages remain in the imported project's archive provenance.
    const REFERENCE_DISPLAY_BYTES: usize = 64 * 1024;
    let mut remaining = REFERENCE_DISPLAY_BYTES;
    let references = imported
        .report
        .external_references
        .into_iter()
        .take_while(|reference| {
            if reference.len() > remaining {
                return false;
            }
            remaining -= reference.len();
            true
        })
        .collect();
    ImportResult {
        project: ImportedProject {
            id: imported.project.id,
            identifier: imported.project.identifier,
            is_public: imported.project.is_public,
        },
        report: ImportReport {
            project: imported.report.project,
            rows: imported
                .report
                .rows
                .into_iter()
                .map(|(table, count)| RowCount {
                    table: table.to_owned(),
                    count,
                })
                .collect(),
            blobs: imported.report.blobs,
            external_references: references,
            external_reference_count: imported.report.external_reference_count,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imported(references: Vec<String>) -> project_archive_import::ImportResponse {
        project_archive_import::ImportResponse {
            project: project_archive_import::ImportedProject {
                id: 7,
                identifier: "ARCIM".into(),
                is_public: false,
            },
            report: project_archive_import::ImportReport {
                project: "ARCIM".into(),
                rows: [("issues", 1)].into_iter().collect(),
                blobs: 0,
                external_reference_count: references.len(),
                external_references: references,
            },
        }
    }

    #[test]
    fn native_project_import_report_bounds_reference_wire_before_shard() {
        let reference = "\0".repeat(64 * 1024);
        let display = report(imported(vec![reference.clone(), "next".into()]));
        assert_eq!(display.report.external_references.len(), 1);
        assert!(display.report.external_references[0] == reference);
        assert_eq!(display.report.external_reference_count, 2);
        assert!(serde_json::to_vec(&display.into_surrogate()).unwrap().len() < 2 * 1024 * 1024);
    }

    #[test]
    fn native_project_import_report_keeps_full_count_when_reference_exceeds_budget() {
        let display = report(imported(vec!["x".repeat(2 * 1024 * 1024), "next".into()]));
        assert!(display.report.external_references.is_empty());
        assert_eq!(display.report.external_reference_count, 2);
    }
}
