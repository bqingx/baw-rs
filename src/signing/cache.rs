//! Private on-disk preview cache: a preview is bound to its digest and may be executed
//! only within its TTL. Files live under the wallet directory, opened relative to an
//! owned 0700 directory without following links.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cached {
    pub(super) version: u32,
    pub(super) kind: String,
    pub(super) client_binding: String,
    pub(super) wallet_device: u64,
    pub(super) wallet_inode: u64,
    pub(super) chain: String,
    pub(super) sender: String,
    pub(super) request_id: String,
    pub(super) expires_at: u64,
    pub(super) input: Value,
    pub(super) output: Value,
}
impl Cached {
    pub(super) fn digest(&self) -> Result<String, String> {
        value_sha(&serde_json::to_value(self).map_err(|_| "cannot encode preview binding")?)
    }
}
pub(super) struct Cache {
    pub(super) dir: File,
    pub(super) wallet_device: u64,
    pub(super) wallet_inode: u64,
}
fn cstr(s: &std::ffi::OsStr) -> Result<CString, String> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(s.as_bytes()).map_err(|_| "invalid cache path".into())
}
fn ioerr() -> String {
    "secure preview cache I/O failed".into()
}
fn openat(parent: &File, name: &CString, flags: i32, mode: u32) -> Result<File, String> {
    // SAFETY: valid directory descriptor and NUL-terminated path; returned descriptor is owned.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        return Err(ioerr());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn private(meta: &std::fs::Metadata, dir: bool) -> Result<(), String> {
    // SAFETY: geteuid has no preconditions.
    if meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o7777 != if dir { 0o700 } else { 0o600 }
        || if dir {
            !meta.is_dir()
        } else {
            !meta.is_file() || meta.nlink() != 1
        }
    {
        return Err("preview cache must be owner-only, regular and not linked".into());
    }
    Ok(())
}
impl Cache {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        if path.as_os_str().is_empty() {
            return Err("explicit wallet directory required".into());
        }
        let start = if path.is_absolute() { "/" } else { "." };
        let mut directory = File::open(start).map_err(|_| ioerr())?;
        for part in path.components() {
            match part {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(n) => {
                    directory =
                        openat(&directory, &cstr(n)?, libc::O_RDONLY | libc::O_DIRECTORY, 0)?
                }
                _ => return Err("parent path components are forbidden".into()),
            }
        }
        let meta = directory.metadata().map_err(|_| ioerr())?;
        private(&meta, true)?;
        let name = CString::new("signing-previews").unwrap();
        let made = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if made < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(ioerr());
        }
        if made == 0 {
            // Persist the new cache-directory entry before any consumption marker can
            // authorize an execute (the operation journal alone only binds operation IDs).
            directory.sync_all().map_err(|_| ioerr())?;
        }
        let dir = openat(&directory, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        private(&dir.metadata().map_err(|_| ioerr())?, true)?;
        Ok(Self {
            dir,
            wallet_device: meta.dev(),
            wallet_inode: meta.ino(),
        })
    }
    pub(super) fn name(id: &str) -> CString {
        CString::new(format!("{}.json", sha(id.as_bytes()))).unwrap()
    }
    pub(super) fn read(&self, id: &str) -> Result<Cached, String> {
        let file = openat(
            &self.dir,
            &Self::name(id),
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        )?;
        let meta = file.metadata().map_err(|_| ioerr())?;
        private(&meta, false)?;
        if meta.len() > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_CACHE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ioerr())?;
        if bytes.len() as u64 > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "preview cache is malformed".into())
    }
    pub(super) fn consumed_name(id: &str) -> CString {
        CString::new(format!("{}.consumed", sha(id.as_bytes()))).unwrap()
    }
    pub(super) fn check_unconsumed(&self, id: &str) -> Result<(), String> {
        let name = Self::consumed_name(id);
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let result = unsafe {
            libc::fstatat(
                self.dir.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result == 0 {
            return Err("preview already consumed; never retry execute".into());
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
            return Err(ioerr());
        }
        Ok(())
    }
    pub(super) fn consume(&self, id: &str, digest: &str) -> Result<(), String> {
        let mut marker = openat(
            &self.dir,
            &Self::consumed_name(id),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|_| "preview already consumed or consumption could not be secured")?;
        marker.write_all(digest.as_bytes()).map_err(|_| ioerr())?;
        marker.sync_all().map_err(|_| ioerr())?;
        self.dir.sync_all().map_err(|_| ioerr())
    }
    pub(super) fn store(&self, cached: &Cached) -> Result<(), String> {
        let bytes = serde_json::to_vec(cached).map_err(|_| "cannot serialize preview cache")?;
        if bytes.len() as u64 > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        let name = Self::name(&cached.request_id);
        let tmp = CString::new(format!(".{}.tmp", rand::random::<u128>())).unwrap();
        let mut file = openat(
            &self.dir,
            &tmp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let result: Result<(), String> = (|| {
            file.write_all(&bytes).map_err(|_| ioerr())?;
            file.sync_all().map_err(|_| ioerr())?;
            // Atomic no-clobber publication; an existing differing request ID is never replaced.
            let linked = unsafe {
                libc::linkat(
                    self.dir.as_raw_fd(),
                    tmp.as_ptr(),
                    self.dir.as_raw_fd(),
                    name.as_ptr(),
                    0,
                )
            };
            if linked < 0 {
                return Err("preview requestId cache already exists; refusing overwrite".into());
            }
            Ok(())
        })();
        unsafe {
            libc::unlinkat(self.dir.as_raw_fd(), tmp.as_ptr(), 0);
        }
        result?;
        self.dir.sync_all().map_err(|_| ioerr())
    }
}
