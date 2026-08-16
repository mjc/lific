//! Project-scoped config I/O with root containment checks.

use std::io;
use std::path::{Path, PathBuf};

fn contained_path(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let root = std::fs::canonicalize(root)?;
    let candidate = if path.exists() {
        std::fs::canonicalize(path)?
    } else {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let parent = std::fs::canonicalize(parent)?;
        parent.join(path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "project path has no file name")
        })?)
    };
    if !candidate.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "project config path escapes the project root",
        ));
    }
    Ok(candidate)
}

pub(crate) fn read(root: &Path, path: &Path) -> io::Result<Option<String>> {
    let path = match contained_path(root, path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn replace(
    root: &Path,
    path: &Path,
    contents: &[u8],
    _contains_secret: bool,
) -> io::Result<()> {
    let path = contained_path(root, path).or_else(|error| {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error);
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        contained_path(root, path)
    })?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    #[cfg(unix)]
    if _contains_secret {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    use std::io::Write;
    let mut file = file;
    file.write_all(contents)?;
    file.sync_all()
}
