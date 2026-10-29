//! Directory-descriptor-relative storage. No keyring or default wallet discovery.
use crate::session::{decrypt, derive_key, Credentials};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Component, Path},
};
use zeroize::Zeroizing;

pub(crate) struct Store {
    directory: File,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    client_id: String,
    #[serde(default, deserialize_with = "present_session")]
    session_id: Option<String>,
}
fn present_session<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
impl Store {
    pub(crate) fn open(path: &Path, create: bool) -> Result<Self, String> {
        if path.as_os_str().is_empty() {
            return Err("explicit wallet directory required".into());
        }
        let parts: Vec<_> = path.components().collect();
        if parts
            .iter()
            .any(|p| matches!(p, Component::ParentDir | Component::Prefix(_)))
        {
            return Err("unsafe wallet directory".into());
        }
        let mut directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(if path.is_absolute() { "/" } else { "." })
            .map_err(|_| "cannot open wallet parent")?;
        let normal: Vec<_> = parts
            .iter()
            .filter_map(|p| {
                if let Component::Normal(n) = p {
                    Some(*n)
                } else {
                    None
                }
            })
            .collect();
        if normal.is_empty() {
            return Err("explicit wallet subdirectory required".into());
        }
        for (i, part) in normal.iter().enumerate() {
            let name = CString::new(part.as_bytes()).map_err(|_| "unsafe wallet directory")?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let mut fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0
                && create
                && i + 1 == normal.len()
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
            {
                if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) } != 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST)
                {
                    return Err("cannot create wallet directory".into());
                }
                fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            }
            if fd < 0 {
                return Err("cannot open wallet directory (symlinks forbidden)".into());
            }
            directory = unsafe { File::from_raw_fd(fd) };
        }
        let m = directory
            .metadata()
            .map_err(|_| "cannot inspect wallet directory")?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
            return Err("wallet directory must be owned and private (0700)".into());
        }
        fs2::FileExt::try_lock_exclusive(&directory)
            .map_err(|_| "wallet directory is locked by another operation")?;
        Ok(Self { directory })
    }
    pub(crate) fn read(&self, name: &CStr) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                Ok(None)
            } else {
                Err("cannot open auth file (symlinks forbidden)".into())
            };
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let m = file.metadata().map_err(|_| "cannot inspect auth file")?;
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o7777 != 0o600
            || m.nlink() != 1
            || m.len() > 1024 * 1024
        {
            return Err("auth file must be regular, owned, single-link and private (0600)".into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        (&mut file)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read auth file")?;
        if bytes.len() > 1024 * 1024 {
            return Err("auth file too large".into());
        }
        Ok(Some(bytes))
    }
    pub(crate) fn write(&self, name: &CStr, bytes: &[u8]) -> Result<(), String> {
        // Check any existing target even though rename never follows it.
        self.read(name)?;
        let mut random = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| "secure random generator unavailable")?;
        let temp = CString::new(format!(".auth-{}", hex(&random))).unwrap();
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                temp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err("cannot create private auth file".into());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let result = (|| {
            file.write_all(bytes)
                .map_err(|_| "cannot write auth file")?;
            file.sync_all().map_err(|_| "cannot sync auth file")?;
            if unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    temp.as_ptr(),
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                )
            } != 0
            {
                return Err("cannot atomically replace auth file".into());
            }
            self.directory
                .sync_all()
                .map_err(|_| "cannot sync wallet directory")?;
            Ok(())
        })();
        if result.is_err() {
            unsafe { libc::unlinkat(self.directory.as_raw_fd(), temp.as_ptr(), 0) };
        }
        result
    }
    pub(crate) fn credentials(&self, instance: &str, create: bool) -> Result<Credentials, String> {
        if instance.is_empty() {
            return Err("explicit instance identity required".into());
        }
        let key = derive_key(instance);
        match self.read(c"session.json")? {
            None if create => {
                let mut id = [0u8; 16];
                OsRng
                    .try_fill_bytes(&mut id)
                    .map_err(|_| "secure random generator unavailable")?;
                id[6] = (id[6] & 15) | 64;
                id[8] = (id[8] & 63) | 128;
                let h = hex(&id);
                let client_id = Zeroizing::new(format!(
                    "{}-{}-{}-{}-{}",
                    &h[..8],
                    &h[8..12],
                    &h[12..16],
                    &h[16..20],
                    &h[20..]
                ));
                self.save(instance, &client_id, "")?;
                Ok(Credentials {
                    client_id,
                    session_id: Zeroizing::new(String::new()),
                })
            }
            None => Err("wallet client identity is missing".into()),
            Some(bytes) => {
                if bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{') {
                    return Err("invalid session JSON".into());
                }
                let stored: Stored =
                    serde_json::from_slice(&bytes).map_err(|_| "invalid session JSON")?;
                let client_id = if is_uuid(&stored.client_id) {
                    Zeroizing::new(stored.client_id)
                } else {
                    decrypt(&stored.client_id, &key)?
                };
                if !is_uuid(&client_id) {
                    return Err("invalid stored client identity".into());
                }
                let session_id = match stored.session_id {
                    Some(v) => decrypt(&v, &key)?,
                    None => Zeroizing::new(String::new()),
                };
                validate_cookie(&session_id)?;
                Ok(Credentials {
                    client_id,
                    session_id,
                })
            }
        }
    }
    pub(crate) fn remove(&self, name: &CStr) -> Result<(), String> {
        if self.read(name)?.is_none() {
            return Ok(());
        }
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err("cannot remove auth file".into());
        }
        self.directory
            .sync_all()
            .map_err(|_| "cannot sync wallet directory".into())
    }
    pub(crate) fn baseline(&self) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let bytes = self
            .read(c"session.json")?
            .ok_or("wallet client identity is missing")?;
        Ok(hex(&Sha256::digest(&*bytes)))
    }
    pub(crate) fn save_cas(
        &self,
        instance: &str,
        client: &str,
        session: &str,
        baseline: &str,
    ) -> Result<(), String> {
        if self.baseline()? != baseline {
            return Err(
                "session changed concurrently; refusing stale authentication update".into(),
            );
        }
        self.save(instance, client, session)
    }
    pub(crate) fn save(&self, instance: &str, client: &str, session: &str) -> Result<(), String> {
        validate_cookie(session)?;
        let key = derive_key(instance);
        let stored = Stored {
            client_id: encrypt(client, &key)?,
            session_id: Some(encrypt(session, &key)?),
        };
        let bytes = serde_json::to_vec(&stored).map_err(|_| "cannot serialize auth file")?;
        self.write(c"session.json", &bytes)
    }
}
fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub(crate) fn validate_cookie(s: &str) -> Result<(), String> {
    if s.len() > 16384
        || !s
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b','))
    {
        Err("invalid session credential".into())
    } else {
        Ok(())
    }
}
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn encrypt(text: &str, key: &[u8; 32]) -> Result<String, String> {
    use aes_gcm::{
        aead::{consts::U16, Aead, KeyInit},
        AesGcm, Nonce,
    };
    use base64::Engine;
    let mut iv = [0u8; 16];
    OsRng
        .try_fill_bytes(&mut iv)
        .map_err(|_| "secure random generator unavailable")?;
    let cipher = AesGcm::<aes::Aes256, U16>::new_from_slice(key).map_err(|_| "invalid key")?;
    let mut bytes = iv.to_vec();
    bytes.extend(
        cipher
            .encrypt(Nonce::<U16>::from_slice(&iv), text.as_bytes())
            .map_err(|_| "cannot encrypt credential")?,
    );
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn persistence_replaces_inode_atomically_and_keeps_credentials_encrypted() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("wallet");
        let store = Store::open(&dir, true).unwrap();
        let c = store.credentials("synthetic-instance", true).unwrap();
        let path = dir.join("session.json");
        let old_inode = std::fs::metadata(&path).unwrap().ino();
        store
            .save(
                "synthetic-instance",
                &c.client_id,
                "synthetic-private-session",
            )
            .unwrap();
        assert_ne!(old_inode, std::fs::metadata(&path).unwrap().ino());
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o7777, 0o600);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&*c.client_id));
        assert!(!text.contains("synthetic-private-session"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert_eq!(
            store
                .credentials("synthetic-instance", false)
                .unwrap()
                .session_id
                .as_str(),
            "synthetic-private-session"
        );
        assert!(crate::session::load(&dir, "synthetic-instance").is_ok());
    }
    #[test]
    fn malformed_existing_session_never_becomes_a_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = Store::open(dir.path(), false).unwrap();
        let c = store.credentials("test", true).unwrap();
        let encrypted = encrypt(&c.client_id, &derive_key("test")).unwrap();
        for text in [
            serde_json::json!({"clientId":encrypted,"sessionId":null}).to_string(),
            serde_json::json!({"clientId":encrypted,"sessionId":""}).to_string(),
            serde_json::json!({"clientId":encrypted,"sessionId":42}).to_string(),
            serde_json::json!({"clientId":encrypted,"sessionId":"not-encrypted"}).to_string(),
            "{broken".to_owned(),
            "{}".to_owned(),
        ] {
            std::fs::write(dir.path().join("session.json"), &text).unwrap();
            assert!(
                store.credentials("test", true).is_err(),
                "malformed session accepted"
            );
            assert_eq!(
                std::fs::read_to_string(dir.path().join("session.json")).unwrap(),
                text
            );
        }
    }
    #[test]
    fn stale_login_cannot_overwrite_new_session() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = Store::open(dir.path(), false).unwrap();
        let c = store.credentials("test", true).unwrap();
        let baseline = store.baseline().unwrap();
        store.save("test", &c.client_id, "newer-token").unwrap();
        assert!(store
            .save_cas("test", &c.client_id, "stale-token", &baseline)
            .is_err());
        assert_eq!(
            store
                .credentials("test", false)
                .unwrap()
                .session_id
                .as_str(),
            "newer-token"
        );
    }
    #[test]
    fn new_explicit_directory_gets_private_stable_encrypted_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("new-wallet");
        let first = Store::open(&path, true)
            .unwrap()
            .credentials("test-instance", true)
            .unwrap();
        assert!(first.session_id.is_empty());
        assert_eq!(first.client_id.len(), 36);
        let second = Store::open(&path, false)
            .unwrap()
            .credentials("test-instance", false)
            .unwrap();
        assert_eq!(*first.client_id, *second.client_id);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        let file = path.join("session.json");
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert!(!std::fs::read_to_string(file)
            .unwrap()
            .contains(first.client_id.as_str()));
        assert!(Store::open(&path, false)
            .unwrap()
            .credentials("wrong-instance", true)
            .is_err());
    }
}
