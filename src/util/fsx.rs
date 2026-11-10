//! Shared filesystem primitives: owner-only metadata checks, descriptor-relative opens and
//! all-or-nothing publication.
//!
//! Nothing here follows a symlink (`O_NOFOLLOW` or `symlink_metadata`) and nothing publishes a
//! partial file: a private temporary file is written, synced, renamed over the target and the
//! directory entry is synced afterwards. A crash therefore leaves either the previous complete
//! file or the new one.
//!
//! Failures are reported as a coarse stage ([`OpenError`], [`Step`]) instead of a message, so
//! every caller keeps its own wording.
use std::{
    ffi::{CStr, CString},
    fs::{File, Metadata, OpenOptions},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::Path,
};

/// The object kind an owner-only check expects; the mode is fixed per kind.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Dir,
    File,
}

/// Shared owner-only rule: the owner is this euid and the mode is exactly 0700 (directory) or
/// 0600 (single-link regular file). Callers add their own wording and any extra limits.
pub(crate) fn check_private_meta(meta: &Metadata, kind: Kind) -> bool {
    // SAFETY: geteuid has no preconditions.
    let owner = meta.uid() == unsafe { libc::geteuid() };
    match kind {
        Kind::Dir => owner && meta.mode() & 0o7777 == 0o700 && meta.is_dir(),
        Kind::File => owner && meta.mode() & 0o7777 == 0o600 && meta.is_file() && meta.nlink() == 1,
    }
}

/// Ownership/type check of a path that is never followed (a symlink is rejected).
pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "cannot inspect private directory")?;
    if !check_private_meta(&m, Kind::Dir) {
        return Err("directory must be owned, private (0700), and not a symlink".into());
    }
    Ok(())
}

/// Open an existing private file for reading; with `create` it may be created (0600).
pub(crate) fn private_file(path: &Path, create: bool) -> Result<File, String> {
    let f = OpenOptions::new()
        .read(true)
        .write(create)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open private file")?;
    let m = f.metadata().map_err(|_| "cannot inspect private file")?;
    if !check_private_meta(&m, Kind::File) {
        return Err("file must be owned, private (0600), single-link and regular".into());
    }
    Ok(f)
}

/// Create `path` (and any missing ancestors) with 0700, then verify the leaf is private.
pub(crate) fn create_private_dir(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok() {
        return private_dir(path);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if std::fs::symlink_metadata(parent).is_err() {
            create_private_dir(parent)?;
        }
    }
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| format!("cannot create directory {}", path.display()))?;
    private_dir(path)
}

/// Open an existing directory without following links; ownership is not checked here.
pub(crate) fn open_dir(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

/// Open an existing directory and require it to be an owned 0700 directory (not a symlink).
pub(crate) fn open_private_dir(path: &Path) -> Result<File, String> {
    let file = open_dir(path).map_err(|_| "cannot open wallet directory")?;
    let meta = file
        .metadata()
        .map_err(|_| "cannot inspect wallet directory")?;
    if !check_private_meta(&meta, Kind::Dir) {
        return Err("wallet directory must be owned and private (0700)".into());
    }
    Ok(file)
}

/// `openat` relative to `dir`, always with `O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK`.
pub(crate) fn open_at(dir: RawFd, name: &CStr, flags: i32, mode: u32) -> std::io::Result<File> {
    // SAFETY: `dir` is a live directory descriptor and `name` is NUL-terminated; the returned
    // descriptor is owned by the caller through `File`.
    let fd = unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            mode,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Which step of an open/verify sequence failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OpenError {
    /// `openat` itself failed (missing, wrong flags, symlink, or already existing).
    Open,
    /// The opened descriptor could be neither inspected nor accepted as private.
    Inspect,
}

/// Open `name` relative to `dir` and require an owned 0600 single-link regular file.
pub(crate) fn open_private_file_at(
    dir: RawFd,
    name: &CStr,
    flags: i32,
    mode: u32,
) -> Result<File, OpenError> {
    let file = open_at(dir, name, flags, mode).map_err(|_| OpenError::Open)?;
    let meta = file.metadata().map_err(|_| OpenError::Inspect)?;
    if !check_private_meta(&meta, Kind::File) {
        return Err(OpenError::Inspect);
    }
    Ok(file)
}

/// Which step of a create/publish sequence failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    /// The temporary or claim file could not be created.
    Open,
    /// The new descriptor could not be inspected or was not private.
    Inspect,
    Write,
    Sync,
    /// The rename over the target failed; the previous file is still in place.
    Publish,
    /// The new directory entry exists but is not durable yet.
    SyncDir,
}

fn temp_name() -> CString {
    CString::new(format!(".baw-rs-tmp-{:032x}", rand::random::<u128>())).expect("ASCII temp name")
}

/// Replace `dir/name` with `bytes`: private temporary file, file sync, rename, directory sync.
/// The target is never truncated in place, so a reader sees the previous or the new content.
pub(crate) fn atomic_write(dir: &File, name: &CStr, bytes: &[u8]) -> Result<(), Step> {
    let fd = dir.as_raw_fd();
    let tmp = temp_name();
    let temp = open_at(
        fd,
        &tmp,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )
    .map_err(|_| Step::Open)?;
    let written = (|| {
        let meta = temp.metadata().map_err(|_| Step::Inspect)?;
        if !check_private_meta(&meta, Kind::File) {
            return Err(Step::Inspect);
        }
        (&temp).write_all(bytes).map_err(|_| Step::Write)?;
        temp.sync_all().map_err(|_| Step::Sync)?;
        // SAFETY: both names are NUL-terminated and relative to the same live directory.
        if unsafe { libc::renameat(fd, tmp.as_ptr(), fd, name.as_ptr()) } != 0 {
            return Err(Step::Publish);
        }
        Ok(())
    })();
    drop(temp);
    if let Err(step) = written {
        // SAFETY: removing a name this call created; a successful rename already consumed it.
        unsafe { libc::unlinkat(fd, tmp.as_ptr(), 0) };
        return Err(step);
    }
    dir.sync_all().map_err(|_| Step::SyncDir)
}

/// Create `dir/name` exactly once, write `bytes`, sync the file and the directory, and return
/// the open file. `Ok(None)` means the name already existed.
///
/// `O_EXCL` creation is the at-most-once claim used by notification delivery, operation ids and
/// preview consumption: an existing name is never overwritten. A failed write leaves the name
/// claimed on purpose, so a retry cannot mistake a partial claim for a free one.
pub(crate) fn create_private_once(
    dir: &File,
    name: &CStr,
    bytes: &[u8],
) -> Result<Option<File>, Step> {
    let fd = dir.as_raw_fd();
    let file = match open_at(
        fd,
        name,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    ) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
        Err(_) => return Err(Step::Open),
    };
    let meta = file.metadata().map_err(|_| Step::Inspect)?;
    if !check_private_meta(&meta, Kind::File) {
        return Err(Step::Inspect);
    }
    (&file).write_all(bytes).map_err(|_| Step::Write)?;
    file.sync_all().map_err(|_| Step::Sync)?;
    dir.sync_all().map_err(|_| Step::SyncDir)?;
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn private_meta_requires_exact_mode_owner_type_and_one_link() {
        let dir = dir();
        let root = fsx_root(&dir);
        let meta = fsx_meta(&root);
        assert!(check_private_meta(&meta, Kind::Dir));
        assert!(!check_private_meta(&meta, Kind::File));

        let file = root.join("record.json");
        std::fs::write(&file, b"{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let meta = fsx_meta(&file);
        assert!(check_private_meta(&meta, Kind::File));
        assert!(!check_private_meta(&meta, Kind::Dir));

        for mode in [0o644, 0o400, 0o666, 0o700] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(
                !check_private_meta(&fsx_meta(&file), Kind::File),
                "accepted file mode {mode:o}"
            );
        }
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!check_private_meta(&fsx_meta(&root), Kind::Dir));
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temporary_file() {
        let dir = dir();
        let root = fsx_root(&dir);
        let handle = open_private_dir(&root).unwrap();
        let name = c"state.json";
        atomic_write(&handle, name, b"one").unwrap();
        assert_eq!(std::fs::read(root.join("state.json")).unwrap(), b"one");
        atomic_write(&handle, name, b"two").unwrap();
        assert_eq!(std::fs::read(root.join("state.json")).unwrap(), b"two");
        let meta = fsx_meta(&root.join("state.json"));
        assert!(check_private_meta(&meta, Kind::File));
        let names: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
    }

    #[test]
    fn create_private_once_is_a_claim_and_never_overwrites() {
        let dir = dir();
        let root = fsx_root(&dir);
        let handle = open_private_dir(&root).unwrap();
        let name = c"claim.json";
        let first = create_private_once(&handle, name, b"claimed").unwrap();
        assert!(first.is_some());
        assert!(create_private_once(&handle, name, b"other")
            .unwrap()
            .is_none());
        assert_eq!(std::fs::read(root.join("claim.json")).unwrap(), b"claimed");
    }

    #[test]
    fn private_checks_reject_loose_modes_and_symlinks() {
        let dir = dir();
        let root = fsx_root(&dir);
        let file = root.join("secret");
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(private_file(&file, false).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(private_file(&file, false).is_ok());
        let link = root.join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(private_file(&link, false).is_err());
        assert!(private_dir(&link).is_err());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_dir(&root).is_err());
        assert!(open_private_dir(&root).is_err());
    }

    fn fsx_root(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().to_path_buf()
    }

    fn fsx_meta(path: &Path) -> Metadata {
        std::fs::symlink_metadata(path).unwrap()
    }
}
