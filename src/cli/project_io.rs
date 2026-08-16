//! Project-scoped config I/O with root containment checks.

use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

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
    let mut file = match open_checked(&path, false, false) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut contents = String::new();
    let bytes = Read::by_ref(&mut file)
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut contents)?;
    if bytes as u64 > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "project config exceeds the 1 MiB limit",
        ));
    }
    Ok(Some(contents))
}

pub(crate) fn replace(
    root: &Path,
    path: &Path,
    contents: &[u8],
    _contains_secret: bool,
) -> io::Result<()> {
    if !root.exists() {
        std::fs::create_dir_all(root)?;
    }
    let path = contained_path(root, path).or_else(|error| {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error);
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let existing_parent = nearest_existing_parent(parent)?;
        contained_path(root, existing_parent)?;
        std::fs::create_dir_all(parent)?;
        contained_path(root, path)
    })?;
    let mut file = open_checked(&path, true, _contains_secret)?;
    file.set_len(0)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn nearest_existing_parent(path: &Path) -> io::Result<&Path> {
    let mut current = path;
    while !current.exists() {
        current = current.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "project config parent does not exist",
            )
        })?;
    }
    Ok(current)
}

fn open_checked(path: &Path, write: bool, _contains_secret: bool) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(!write).write(write).create(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let flags = libc::O_NOFOLLOW | if write { 0 } else { libc::O_NONBLOCK };
        options.custom_flags(flags);
        if write {
            options.mode(if _contains_secret { 0o600 } else { 0o666 });
        }
    }
    #[cfg(not(unix))]
    if path.exists() {
        reject_reparse(path)?;
    }

    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "project config must be a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "project config must not be hard-linked",
            ));
        }
        if write && _contains_secret {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(file)
}

#[cfg(not(unix))]
fn reject_reparse(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "project config must not be a symbolic link",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "project config must not use a Windows reparse point",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn read_rejects_fifo_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("config");
        let fifo_name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        let rc = unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) };
        assert_eq!(rc, 0);
        let error = read(dir.path(), &fifo).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("..").join("outside-config");
        let error = replace(dir.path(), &outside, b"{}", false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
