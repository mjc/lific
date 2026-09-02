//! Small, shared filesystem primitives for private application files.

use std::path::{Path, PathBuf};

pub(crate) fn parent_or_dot(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

pub(crate) fn ensure_private_parent(path: &Path) -> std::io::Result<()> {
    let parent = parent_or_dot(path);
    let existed = parent.exists();
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    if !existed {
        set_private_dir(parent)?;
    }
    Ok(())
}

pub(crate) fn atomic_create_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    atomic_write_private(path, contents, PublishMode::Create)
}

pub(crate) fn atomic_replace_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    atomic_write_private(path, contents, PublishMode::Replace)
}

fn atomic_write_private(path: &Path, contents: &[u8], publish: PublishMode) -> std::io::Result<()> {
    use std::io::Write;

    ensure_private_parent(path)?;
    let parent = parent_or_dot(path);
    let staging = tempfile::Builder::new()
        .prefix(".lific-private-")
        .tempdir_in(parent)?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "private file path must name a file",
        )
    })?;
    let temp = staging.path().join(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&temp)?;
    file.write_all(contents)?;
    set_private_file(&file)?;
    file.sync_all()?;

    match publish {
        PublishMode::Create => std::fs::hard_link(&temp, path)?,
        PublishMode::Replace => std::fs::rename(&temp, path)?,
    }
    sync_parent(parent)
}

enum PublishMode {
    Create,
    Replace,
}

pub(crate) fn set_private_file(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
pub(crate) fn set_private_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn sync_parent(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn absolutize(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{absolutize, parent_or_dot};
    use std::path::Path;

    #[test]
    fn relative_paths_are_resolved_once_at_the_boundary() {
        assert_eq!(
            absolutize(Path::new("lific.toml"), Path::new("/srv")),
            Path::new("/srv/lific.toml")
        );
        assert_eq!(
            absolutize(Path::new("/etc/lific.toml"), Path::new("/srv")),
            Path::new("/etc/lific.toml")
        );
    }

    #[test]
    fn single_component_paths_use_the_current_directory_as_parent() {
        assert_eq!(parent_or_dot(Path::new("lific.toml")), Path::new("."));
        assert_eq!(
            parent_or_dot(Path::new("config/lific.toml")),
            Path::new("config")
        );
    }
}
