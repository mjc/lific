//! Shared bounded export preparation and streaming for REST and native routes.

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::IntoResponse;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::OwnedSemaphorePermit;

use crate::{
    authz,
    db::{DbPool, models::Role},
    error::LificError,
    resolve_caller::ResolvedIdentity,
};

pub(crate) const EXPORT_STREAM_CHUNK_BYTES: usize = 64 * 1024;
const EXPORT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const EXPORT_STREAM_MAX_DURATION: Duration = Duration::from_secs(30 * 60);

/// A finished export sitting in a private temp directory, ready to stream.
///
/// LIF-467: crate-visible so the project-archive routes reuse this streaming
/// path (its idle/total deadlines, its temp-dir-owning body, its slot
/// handling) instead of growing a second one.
pub(crate) struct PreparedExport {
    pub(crate) temp_dir: tempfile::TempDir,
    pub(crate) path: std::path::PathBuf,
    pub(crate) content_type: HeaderValue,
    pub(crate) download_name: Option<String>,
    /// Extra response headers this download needs. Empty for the ordinary
    /// bundle exports; the archive download uses it for `Cache-Control:
    /// no-store` and the sniffing/scripting defenses.
    pub(crate) extra_headers: HeaderMap,
}

impl PreparedExport {
    pub(crate) fn json(bundle: &crate::export::ExportBundle) -> Result<Self, LificError> {
        let temp_dir = export_temp_dir()?;
        let path = temp_dir.path().join("export.json");
        crate::export::bundle_to_json_file(bundle, &path)?;
        Ok(Self {
            temp_dir,
            path,
            content_type: HeaderValue::from_static("application/json"),
            download_name: None,
            extra_headers: HeaderMap::new(),
        })
    }

    pub(crate) fn markdown(
        bundle: crate::export::ExportBundle,
        fallback_name: &str,
    ) -> Result<Self, LificError> {
        let file = bundle.files.into_iter().next().ok_or_else(|| {
            LificError::Internal(format!("export produced no files for {fallback_name}"))
        })?;
        let download_name = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(fallback_name)
            .to_string();
        let temp_dir = export_temp_dir()?;
        let path = temp_dir.path().join("export.md");
        std::fs::write(&path, file.content)
            .map_err(|error| LificError::Internal(format!("write export file: {error}")))?;
        Ok(Self {
            temp_dir,
            path,
            content_type: HeaderValue::from_static("text/markdown; charset=utf-8"),
            download_name: Some(download_name),
            extra_headers: HeaderMap::new(),
        })
    }

    pub(crate) fn zip(bundle: &crate::export::ExportBundle) -> Result<Self, LificError> {
        let download_name = format!("{}-export.zip", bundle.root.to_ascii_lowercase());
        let temp_dir = export_temp_dir()?;
        let path = temp_dir.path().join("export.zip");
        crate::export::bundle_to_zip_file(bundle, &path)?;
        Ok(Self {
            temp_dir,
            path,
            content_type: HeaderValue::from_static("application/zip"),
            download_name: Some(download_name),
            extra_headers: HeaderMap::new(),
        })
    }
}

fn export_temp_dir() -> Result<tempfile::TempDir, LificError> {
    tempfile::tempdir()
        .map_err(|error| LificError::Internal(format!("create export temp dir: {error}")))
}

async fn blocking<T>(
    operation: impl FnOnce() -> Result<T, LificError> + Send + 'static,
) -> Result<T, LificError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| LificError::Internal(format!("export worker failed: {error}")))?
}

/// LIF-467: also used by the project-archive tests, which need to hold a
/// blocking worker open between "spawned" and "committed".
#[cfg(test)]
pub(crate) struct ExportTestGate {
    started: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

#[cfg(test)]
impl ExportTestGate {
    pub(crate) fn new(
        started: tokio::sync::oneshot::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) -> Self {
        Self {
            started: std::sync::Mutex::new(Some(started)),
            release: std::sync::Mutex::new(release),
        }
    }

    fn wait(&self) {
        let Some(started) = self.started.lock().unwrap().take() else {
            return;
        };
        let _ = started.send(());
        self.release.lock().unwrap().recv().unwrap();
    }
}

#[cfg(test)]
tokio::task_local! {
    pub(crate) static EXPORT_TEST_GATE: std::sync::Arc<ExportTestGate>;
}

pub(crate) async fn blocking_export<T>(
    permit: OwnedSemaphorePermit,
    operation: impl FnOnce() -> Result<T, LificError> + Send + 'static,
) -> Result<(T, OwnedSemaphorePermit), LificError>
where
    T: Send + 'static,
{
    #[cfg(test)]
    let gate = EXPORT_TEST_GATE.try_with(std::sync::Arc::clone).ok();
    blocking(move || {
        #[cfg(test)]
        if let Some(gate) = gate {
            gate.wait();
        }
        Ok((operation()?, permit))
    })
    .await
}

pub(crate) async fn single_file_response(
    bundle: crate::export::ExportBundle,
    format: &str,
    fallback_name: &'static str,
    permit: OwnedSemaphorePermit,
) -> Result<axum::response::Response, LificError> {
    let (prepared, permit) = match format {
        "json" => blocking_export(permit, move || PreparedExport::json(&bundle)).await?,
        "markdown" => {
            blocking_export(permit, move || {
                PreparedExport::markdown(bundle, fallback_name)
            })
            .await?
        }
        _ => unreachable!("format was validated before export"),
    };
    stream_response(prepared, permit).await
}

pub(crate) async fn stream_response(
    prepared: PreparedExport,
    permit: OwnedSemaphorePermit,
) -> Result<axum::response::Response, LificError> {
    stream_response_with_timeouts(
        prepared,
        permit,
        EXPORT_STREAM_IDLE_TIMEOUT,
        EXPORT_STREAM_MAX_DURATION,
    )
    .await
}

pub(crate) async fn stream_response_with_timeouts(
    prepared: PreparedExport,
    permit: OwnedSemaphorePermit,
    idle_timeout: Duration,
    max_duration: Duration,
) -> Result<axum::response::Response, LificError> {
    let mut file = tokio::fs::File::open(&prepared.path)
        .await
        .map_err(|error| LificError::Internal(format!("open export file: {error}")))?;
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (terminal_sender, terminal_receiver) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _temp_dir = prepared.temp_dir;
        let _permit = permit;
        let mut buffer = vec![0; EXPORT_STREAM_CHUNK_BYTES];
        let result = tokio::time::timeout(max_duration, async {
            loop {
                match file.read(&mut buffer).await {
                    Ok(0) => return Ok(()),
                    Ok(read) => {
                        let chunk = Bytes::copy_from_slice(&buffer[..read]);
                        match tokio::time::timeout(idle_timeout, sender.send(chunk)).await {
                            Ok(Ok(())) => {}
                            Ok(Err(_)) => return Ok(()),
                            Err(_) => {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    "export stream idle timeout",
                                ));
                            }
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        })
        .await;
        let result = result.unwrap_or_else(|_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "export stream deadline exceeded",
            ))
        });
        let _ = terminal_sender.send(result);
    });
    let body = stream_body(receiver, terminal_receiver);
    let mut headers = prepared.extra_headers;
    headers.insert(header::CONTENT_TYPE, prepared.content_type);
    if let Some(filename) = prepared.download_name {
        headers.insert(header::CONTENT_DISPOSITION, content_disposition(&filename)?);
    }
    Ok((headers, body).into_response())
}

pub(crate) fn stream_body(
    receiver: tokio::sync::mpsc::Receiver<Bytes>,
    terminal: tokio::sync::oneshot::Receiver<std::io::Result<()>>,
) -> Body {
    // Compression can poll for trailers after EOF. Unfold alone panics on
    // that second terminal poll; fuse keeps the completed body exhausted.
    Body::from_stream(
        futures_util::stream::unfold(
            (receiver, Some(terminal)),
            |(mut receiver, mut terminal)| async move {
                if let Some(chunk) = receiver.recv().await {
                    return Some((Ok::<_, std::io::Error>(chunk), (receiver, terminal)));
                }
                let result = terminal.take()?.await.unwrap_or_else(|_| {
                    Err(std::io::Error::other(
                        "export stream task ended without a result",
                    ))
                });
                result.err().map(|error| (Err(error), (receiver, terminal)))
            },
        )
        .fuse(),
    )
}

pub(crate) fn content_disposition(filename: &str) -> Result<HeaderValue, LificError> {
    HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|e| LificError::Internal(format!("invalid content-disposition header: {e}")))
}

/// Resolve current Viewer authority before acquiring shared export capacity.
pub(crate) async fn issue(
    db: DbPool,
    identity: &Option<ResolvedIdentity>,
    identifier: String,
    format: Option<String>,
) -> Result<axum::response::Response, LificError> {
    if let Some(format) = format.as_deref()
        && !matches!(format, "json" | "markdown")
    {
        return Err(LificError::BadRequest(
            "invalid export format. Expected 'markdown' or 'json'".into(),
        ));
    }
    let project_id = {
        let conn = db.read()?;
        let id = crate::db::queries::resolve_identifier(&conn, &identifier)?;
        crate::db::queries::issue_project_id(&conn, id)?
    };
    authz::require_role(&db, identity, project_id, Role::Viewer)?;
    let visible = authz::visible_project_ids(&db, identity)?;
    let slot = db.acquire_export_slot()?;
    let (bundle, slot) = blocking_export(slot, move || {
        let conn = db.read()?;
        crate::export::export_issue(&conn, &identifier, visible.as_ref())
    })
    .await?;
    single_file_response(
        bundle,
        format.as_deref().unwrap_or("markdown"),
        "issue.md",
        slot,
    )
    .await
}
