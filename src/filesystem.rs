use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// Reject a path that contains a symlink in any existing component.
pub(crate) fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::CurDir) {
            continue;
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(unsafe_path(path, "must not contain a symlink"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Create a directory tree with owner-only permissions where the platform
/// supports them. Existing group/other-writable directories are rejected.
pub(crate) fn ensure_private_dir(path: &Path) -> io::Result<()> {
    ensure_private_parent(path)?;
    set_private_dir(path)
}

/// Create a private parent when absent, or validate an existing parent without
/// changing a safe, traversable mode such as 0755.
pub(crate) fn ensure_private_parent(path: &Path) -> io::Result<()> {
    let existed = symlink_metadata_if_exists(path)?.is_some();
    ensure_dir(path)?;
    if existed {
        reject_group_writable(path)
    } else {
        set_private_dir(path)
    }
}

/// Create a directory tree after checking every existing component for
/// symlinks. This is for non-secret output directories that should retain their
/// normal permissions.
pub(crate) fn ensure_dir(path: &Path) -> io::Result<()> {
    reject_symlink_ancestors(path)?;
    fs::create_dir_all(path)?;
    reject_symlink_ancestors(path)
}

/// Validate a directory that will receive private output. Sticky directories
/// are allowed because they protect entries from other users' renames.
pub(crate) fn validate_private_parent(path: &Path) -> io::Result<()> {
    reject_symlink_ancestors(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = fs::symlink_metadata(path)?.mode();
        if mode & 0o1000 == 0 && mode & 0o022 != 0 {
            return Err(unsafe_path(
                path,
                "is group/world-writable without sticky protection",
            ));
        }
    }
    Ok(())
}

/// Open an existing file without following symlinks.
pub(crate) fn open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    open_with_options(&mut options, path)
}

fn open_with_options(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    reject_symlink_ancestors(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        // FILE_FLAG_OPEN_REPARSE_POINT prevents Windows from traversing a
        // final reparse point when opening the file.
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    if is_link(&file.metadata()?) {
        return Err(unsafe_path(path, "must not be a symlink"));
    }
    Ok(file)
}

/// Create a new owner-only file without following symlinks.
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = open_with_options(&mut options, path)?;
    set_private_file(&file)?;
    Ok(file)
}

/// Create a private temporary file in `parent`, removed automatically unless
/// its path is published elsewhere.
pub(crate) fn private_tempfile_in(
    parent: &Path,
    prefix: &str,
) -> io::Result<tempfile::NamedTempFile> {
    reject_symlink_ancestors(parent)?;
    let file = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(parent)?;
    set_private_file(file.as_file())?;
    Ok(file)
}

/// Tighten permissions on an already-open file without reopening its path.
fn set_private_file(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

/// Tighten permissions on a file path without first reopening it through a
/// possibly swapped symlink.
pub(crate) fn set_private_file_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut options = OpenOptions::new();
        options.read(true);
        let file = open_with_options(&mut options, path)?;
        set_private_file(&file)?;
    }
    #[cfg(not(unix))]
    reject_symlink(path)?;
    Ok(())
}

/// Tighten permissions on an existing directory after verifying it is not a
/// symlink. Directory permission changes are a no-op on Windows.
fn set_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut options = OpenOptions::new();
        options.read(true);
        let directory = open_with_options(&mut options, path)?;
        directory.set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    reject_symlink(path)?;
    Ok(())
}

/// Publish a fully-written temporary file over a regular destination.
/// Existing symlinks and hard-linked files are refused before publication.
pub(crate) fn atomic_replace(temp: &Path, destination: &Path) -> io::Result<()> {
    reject_symlink_ancestors(destination)?;
    safe_destination_exists(destination)?;

    #[cfg(not(windows))]
    {
        fs::rename(temp, destination)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };

        let wide = |path: &Path| -> io::Result<Vec<u16>> {
            let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
            if wide.contains(&0) {
                return Err(unsafe_path(path, "must not contain NUL"));
            }
            wide.push(0);
            Ok(wide)
        };
        let temp = wide(temp)?;
        let destination = wide(destination)?;
        // SAFETY: both vectors are NUL-terminated UTF-16 paths owned for the
        // duration of the call, and MoveFileExW does not retain the pointers.
        let ok = unsafe {
            MoveFileExW(
                temp.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

pub(crate) fn safe_destination_exists(path: &Path) -> io::Result<bool> {
    Ok(destination_metadata(path)?.is_some())
}

pub(crate) fn safe_path_exists(path: &Path) -> io::Result<bool> {
    Ok(safe_metadata(path)?.is_some())
}

#[cfg(not(unix))]
fn reject_symlink(path: &Path) -> io::Result<()> {
    match safe_metadata(path)? {
        Some(_) => Ok(()),
        None => Err(io::ErrorKind::NotFound.into()),
    }
}

/// Write bytes to a private staging file and publish them in one operation.
pub(crate) fn write_atomic(path: &Path, contents: &[u8], private: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        ensure_dir(parent)?;
    }
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".lific-write-")
        .tempdir_in(parent)?;
    let temp = staging.path().join(path.file_name().unwrap_or_default());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if !private {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let mode = destination_metadata(path)?
            .map(|metadata| metadata.mode() & 0o777)
            .filter(|mode| *mode != 0)
            .unwrap_or(0o666);
        options.mode(mode);
    }
    let mut file = if private {
        create_private(&temp)?
    } else {
        open_with_options(&mut options, &temp)?
    };
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    atomic_replace(&temp, path)
}

fn reject_group_writable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = fs::symlink_metadata(path)?.mode() & 0o777;
        if mode & 0o022 != 0 {
            return Err(unsafe_path(
                path,
                "is writable by group/others; remove group/other write permissions",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn destination_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    let Some(metadata) = safe_metadata(path)? else {
        return Ok(None);
    };
    if number_of_links(&metadata) > 1 {
        return Err(unsafe_path(path, "must not be a hard link"));
    }
    Ok(Some(metadata))
}

fn safe_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    let metadata = symlink_metadata_if_exists(path)?;
    if metadata.as_ref().is_some_and(is_link) {
        return Err(unsafe_path(path, "must not be a symlink"));
    }
    Ok(metadata)
}

fn symlink_metadata_if_exists(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn number_of_links(metadata: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.nlink()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        u64::from(metadata.number_of_links().unwrap_or(1))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        1
    }
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

fn unsafe_path(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("path {} {reason}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_replace, create_private, ensure_private_dir, open, private_tempfile_in,
        safe_destination_exists, safe_path_exists,
    };

    #[test]
    fn private_directory_rejects_symlinked_ancestor() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let link = root.path().join("link");
        std::fs::create_dir(&real).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert!(ensure_private_dir(&link.join("child")).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_open_rejects_final_symlink() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let link = root.path().join("link");
        std::fs::write(&target, "secret").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(open(&link).is_err());
        assert!(safe_path_exists(&link).is_err());
    }

    #[test]
    fn atomic_replace_publishes_new_contents() {
        let root = tempfile::tempdir().unwrap();
        let temp = root.path().join("temp");
        let destination = root.path().join("destination");
        std::fs::write(&temp, "new").unwrap();
        std::fs::write(&destination, "old").unwrap();

        atomic_replace(&temp, &destination).unwrap();

        assert_eq!(std::fs::read_to_string(destination).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_rejects_hard_link_destination() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing");
        let destination = root.path().join("destination");
        let temp = root.path().join("temp");
        std::fs::write(&existing, "old").unwrap();
        std::fs::hard_link(&existing, &destination).unwrap();
        std::fs::write(&temp, "new").unwrap();

        assert!(atomic_replace(&temp, &destination).is_err());
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "old");
    }

    #[test]
    fn destination_check_propagates_metadata_errors() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, "not a directory").unwrap();

        assert!(safe_destination_exists(&file.join("child")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn private_open_applies_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private");
        let file = create_private(&path).unwrap();

        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn private_temporary_file_applies_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let file = private_tempfile_in(root.path(), "private-").unwrap();

        assert_eq!(
            file.as_file().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
