use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Reject a path that contains a symlink in any existing component.
pub(crate) fn reject_symlink_ancestors(path: &Path) -> io::Result<()> {
    inspect_ancestors(path, false)
}

fn reject_unsafe_ancestors(path: &Path) -> io::Result<()> {
    inspect_ancestors(path, true)
}

fn inspect_ancestors(path: &Path, reject_writable: bool) -> io::Result<()> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(unsafe_path(path, "must not contain parent traversal"));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        if component == Component::CurDir {
            continue;
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link(&metadata) => {
                return Err(unsafe_path(path, "must not contain a symlink"));
            }
            Ok(metadata) => {
                #[cfg(unix)]
                if reject_writable && metadata.is_dir() {
                    use std::os::unix::fs::MetadataExt;
                    let mode = metadata.mode();
                    if mode & 0o1000 == 0 && mode & 0o022 != 0 {
                        return Err(unsafe_path(
                            &current,
                            "is group/world-writable without sticky protection",
                        ));
                    }
                }
                #[cfg(not(unix))]
                let _ = (&metadata, reject_writable);
            }
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
    reject_group_writable(path)?;
    set_private_dir(path)
}

/// Create a private parent when absent, or validate an existing parent without
/// changing a safe, traversable mode such as 0755.
pub(crate) fn ensure_private_parent(path: &Path) -> io::Result<()> {
    reject_unsafe_ancestors(path)?;
    create_private_dir_all(path)?;
    reject_unsafe_ancestors(path)
}

#[cfg(unix)]
fn create_private_dir_all(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_dir_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
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
    reject_unsafe_ancestors(path)?;
    match symlink_metadata_if_exists(path)? {
        Some(metadata) if metadata.is_dir() => Ok(()),
        Some(_) => Err(unsafe_path(path, "must be a directory")),
        None => Err(io::ErrorKind::NotFound.into()),
    }
}

/// Open an existing file without following symlinks.
pub(crate) fn open(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    open_with_options(&mut options, path)
}

pub(crate) fn open_private_with_options(
    options: &mut OpenOptions,
    path: &Path,
) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    open_with_options(options, path)
}

pub(crate) fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn read_to_string(path: &Path) -> io::Result<String> {
    let mut contents = String::new();
    open(path)?.read_to_string(&mut contents)?;
    Ok(contents)
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
    open_private_with_options(&mut options, path)
}

/// Create a private temporary file in `parent`, removed automatically unless
/// its path is published elsewhere.
pub(crate) fn private_tempfile_in(
    parent: &Path,
    prefix: &str,
) -> io::Result<tempfile::NamedTempFile> {
    reject_symlink_ancestors(parent)?;
    tempfile::Builder::new().prefix(prefix).tempfile_in(parent)
}

/// Tighten permissions on a file path without first reopening it through a
/// possibly swapped symlink.
pub(crate) fn set_private_file_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut options = OpenOptions::new();
        options.read(true);
        let file = open_with_options(&mut options, path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    reject_symlink(path)?;
    Ok(())
}

#[cfg_attr(not(unix), expect(clippy::unnecessary_wraps, reason = "fallible on Unix"))]
pub(crate) fn set_private_file(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if file.metadata()?.mode() & 0o077 != 0 {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
    }
    #[cfg(not(unix))]
    let _ = file;
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

/// Publish a fully-written temporary file over a destination without following
/// symlinks. Replacing a hard link safely detaches that directory entry from
/// the old inode.
pub(crate) fn atomic_replace(temp: &Path, destination: &Path) -> io::Result<()> {
    safe_path_exists(destination)?;

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

pub(crate) fn safe_path_exists(path: &Path) -> io::Result<bool> {
    Ok(safe_metadata(path)?.is_some())
}

pub(crate) fn safe_regular_file_exists(path: &Path) -> io::Result<bool> {
    let Some(metadata) = safe_metadata(path)? else {
        return Ok(false);
    };
    if !metadata.file_type().is_file() {
        return Err(unsafe_path(path, "must be a regular file"));
    }
    if link_count(path, &metadata)? != 1 {
        return Err(unsafe_path(path, "must not be a hard link"));
    }
    Ok(true)
}

/// Reject an existing file with multiple names. Dump output uses this stricter
/// policy so it cannot replace only one name for a file the user may expect to
/// remain linked.
pub(crate) fn reject_hard_link(path: &Path) -> io::Result<()> {
    if let Some(metadata) = safe_metadata(path)?
        && link_count(path, &metadata)? > 1
    {
        return Err(unsafe_path(path, "must not be a hard link"));
    }
    Ok(())
}

/// Reject an already-open file with multiple names without resolving its path
/// again.
pub(crate) fn reject_hard_link_file(path: &Path, file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if file_link_count(file, &metadata)? > 1 {
        return Err(unsafe_path(path, "must not be a hard link"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn reject_symlink(path: &Path) -> io::Result<()> {
    match safe_metadata(path)? {
        Some(_) => Ok(()),
        None => Err(io::ErrorKind::NotFound.into()),
    }
}

/// Write bytes atomically, preserving an existing file's mode where supported.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_atomic_with_mode(path, contents, false)
}

/// Write bytes atomically to an owner-only file and secure newly-created parent
/// directories.
pub(crate) fn write_private_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_atomic_with_mode(path, contents, true)
}

fn write_atomic_with_mode(path: &Path, contents: &[u8], private: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        if private {
            ensure_private_parent(parent)?;
        } else {
            ensure_dir(parent)?;
        }
    }
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".lific-write-")
        .tempdir_in(parent)?;
    let temp = staging.path().join(path.file_name().unwrap_or_default());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    let preserved_mode = if private {
        None
    } else {
        use std::os::unix::fs::MetadataExt;
        safe_metadata(path)?.map(|metadata| metadata.mode() & 0o777)
    };
    let mut file = if private {
        create_private(&temp)?
    } else {
        open_with_options(&mut options, &temp)?
    };
    #[cfg(unix)]
    if let Some(mode) = preserved_mode {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    atomic_replace(&temp, path)?;
    sync_dir(parent)
}

#[cfg_attr(not(unix), expect(clippy::unnecessary_wraps, reason = "fallible on Unix"))]
pub(crate) fn sync_dir(_path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(_path)?.sync_all()?;
    Ok(())
}

#[cfg_attr(not(unix), expect(clippy::unnecessary_wraps, reason = "fallible on Unix"))]
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

fn safe_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    reject_symlink_ancestors(path)?;
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

#[cfg_attr(
    not(windows),
    expect(
        clippy::unnecessary_wraps,
        reason = "Windows link inspection can fail and must fail closed"
    )
)]
fn link_count(path: &Path, metadata: &Metadata) -> io::Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(metadata.nlink())
    }
    #[cfg(windows)]
    {
        let file = open(path)?;
        file_link_count(&file, metadata)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, metadata);
        Ok(1)
    }
}

#[cfg_attr(
    not(windows),
    expect(
        clippy::unnecessary_wraps,
        reason = "Windows handle inspection can fail and must fail closed"
    )
)]
fn file_link_count(file: &File, metadata: &Metadata) -> io::Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = file;
        Ok(metadata.nlink())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };

        let _ = metadata;
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `information` is a valid writable struct and the handle
        // remains open for the duration of the call.
        let result = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(u64::from(information.nNumberOfLinks))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, metadata);
        Ok(1)
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
    use super::{atomic_replace, reject_hard_link_file, safe_path_exists, validate_private_parent};

    #[cfg(unix)]
    use super::{create_private, ensure_private_dir, open, private_tempfile_in};

    #[cfg(unix)]
    #[test]
    fn private_directory_rejects_symlinked_ancestor() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let link = root.path().join("link");
        std::fs::create_dir(&real).unwrap();

        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(ensure_private_dir(&link.join("child")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn directory_creation_rejects_parent_traversal_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(elsewhere.path(), &link).unwrap();
        let path = root.path().join("missing/../link/child");

        assert!(super::ensure_dir(&path).is_err());
        assert!(!elsewhere.path().join("child").exists());
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_rejects_writable_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();

        assert!(ensure_private_dir(&shared.join("private")).is_err());
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

    #[test]
    fn existing_private_parent_is_required() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");

        assert_eq!(
            validate_private_parent(&missing).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn open_hard_link_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let linked = root.path().join("linked");
        std::fs::write(&original, "contents").unwrap();
        std::fs::hard_link(&original, &linked).unwrap();
        let file = std::fs::File::open(&linked).unwrap();

        assert!(reject_hard_link_file(&linked, &file).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_detaches_hard_link_destination() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing");
        let destination = root.path().join("destination");
        let temp = root.path().join("temp");
        std::fs::write(&existing, "old").unwrap();
        std::fs::hard_link(&existing, &destination).unwrap();
        std::fs::write(&temp, "new").unwrap();

        atomic_replace(&temp, &destination).unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_exactly() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("destination");
        std::fs::write(&destination, "old").unwrap();
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o620)).unwrap();

        super::write_atomic(&destination, b"new").unwrap();

        assert_eq!(
            std::fs::metadata(destination).unwrap().permissions().mode() & 0o777,
            0o620
        );
    }

    #[test]
    fn destination_check_propagates_metadata_errors() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, "not a directory").unwrap();

        assert!(safe_path_exists(&file.join("child")).is_err());
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
    fn private_parent_creates_owner_only_directory_tree() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let intermediate = root.path().join("intermediate");
        let parent = intermediate.join("parent");

        super::ensure_private_parent(&parent).unwrap();

        for path in [intermediate, parent] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
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
