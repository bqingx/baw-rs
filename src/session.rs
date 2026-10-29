//! Read-only compatibility with explicit BAW file sessions.
use zeroize::Zeroizing;

pub struct Credentials {
    pub client_id: Zeroizing<String>,
    pub session_id: Zeroizing<String>,
}

/// Derive the CLI-compatible key; the instance identity must match session creation.
pub fn derive_key(instance: &str) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0; 32]);
    let salt = Zeroizing::new(format!("baw-agentic-wallet-v1:{instance}"));
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(instance.as_bytes(), salt.as_bytes(), 100_000, &mut *key);
    key
}
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> Result<Zeroizing<String>, String> {
    use aes_gcm::{
        aead::{consts::U16, AeadInPlace, KeyInit},
        AesGcm, Nonce, Tag,
    };
    use base64::Engine;
    type Cipher = AesGcm<aes::Aes256, U16>;
    let bytes = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "invalid encrypted credential".to_owned())?,
    );
    if bytes.len() < 32 {
        return Err("invalid encrypted credential".into());
    }
    let split = bytes.len() - 16;
    let mut plain = Zeroizing::new(bytes[16..split].to_vec());
    let cipher = Cipher::new_from_slice(key).map_err(|_| "invalid key".to_owned())?;
    cipher
        .decrypt_in_place_detached(
            Nonce::<U16>::from_slice(&bytes[..16]),
            b"",
            &mut plain,
            Tag::from_slice(&bytes[split..]),
        )
        .map_err(|_| "credential authentication failed".to_owned())?;
    let text = std::str::from_utf8(&plain).map_err(|_| "invalid credential text".to_owned())?;
    Ok(Zeroizing::new(text.to_owned()))
}
/// Read only `dir/session.json`, requiring owner-only directory/file permissions.
/// No keyring, identity discovery, fallback session, or credential migration is attempted.
/// Client identity is nonempty; an authenticated encrypted empty session represents logout.
pub fn load(dir: &std::path::Path, instance: &str) -> Result<Credentials, String> {
    use std::{
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
    };
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(dir.components().collect::<std::path::PathBuf>())
        .map_err(|_| "cannot open session directory".to_owned())?;
    let metadata = directory
        .metadata()
        .map_err(|_| "cannot inspect session directory".to_owned())?;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err("session path must be owned by current user".into());
    }
    if metadata.mode() & 0o7777 != 0o700 {
        return Err("session directory must be private (0700)".into());
    }
    // Open relative to the checked directory descriptor, never re-resolve its path.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c"session.json".as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err("cannot open session file".into());
    }
    // SAFETY: openat returned an owned valid descriptor, consumed exactly once.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect session file".to_owned())?;
    if !metadata.is_file() {
        return Err("session file must be regular".into());
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err("session path must be owned by current user".into());
    }
    if metadata.mode() & 0o7777 != 0o600 {
        return Err("session file must be private (0600)".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.read_to_end(&mut bytes)
        .map_err(|_| "cannot read session file".to_owned())?;
    fn secret<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Zeroizing<String>, D::Error> {
        <String as serde::Deserialize>::deserialize(deserializer).map(Zeroizing::new)
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Stored {
        #[serde(deserialize_with = "secret")]
        client_id: Zeroizing<String>,
        #[serde(deserialize_with = "secret")]
        session_id: Zeroizing<String>,
    }
    if bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{') {
        return Err("session must be a JSON object".into());
    }
    let stored: Stored =
        serde_json::from_slice(&bytes).map_err(|_| "invalid session JSON".to_owned())?;
    let key = derive_key(instance);
    let client_id = if is_uuid(&stored.client_id) {
        stored.client_id
    } else {
        decrypt(&stored.client_id, &key)?
    };
    let session_id = decrypt(&stored.session_id, &key)?;
    for value in [&client_id, &session_id] {
        if !value.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err("invalid authentication value".into());
        }
    }
    if !is_uuid(&client_id) {
        return Err("invalid client identity".into());
    }
    Ok(Credentials {
        client_id,
        session_id,
    })
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    const INSTANCE: &str = "synthetic-baw-test-instance";
    // Generated independently by Python hashlib.pbkdf2_hmac + cryptography AESGCM.
    const SESSION: &str =
        "AAECAwQFBgcICQoLDA0OD5bjeqbdyeBoraR8XA6uMlE0+39LF7Kj+vOLW2/aVI4LHlNiZ6em1Q==";
    const CLIENT: &str = "AAECAwQFBgcICQoLDA0OD9WrJuGBmaI247E2WB/wb10+syYcGravMhgOBVDb4bkOGwPIsVM440ZVQwfNY3XN3ROmvwo=";
    const UUID: &str = "01234567-89ab-4cde-8fab-0123456789ab";

    fn fixture(client: &str) -> tempfile::TempDir {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.path().join("session.json"))
            .unwrap();
        write!(
            file,
            "{}",
            serde_json::json!({"clientId":client,"sessionId":SESSION})
        )
        .unwrap();
        dir
    }

    fn encrypt_test(value: &[u8]) -> String {
        use aes_gcm::{
            aead::{consts::U16, Aead, KeyInit},
            AesGcm, Nonce,
        };
        use base64::Engine;
        let key = derive_key(INSTANCE);
        let cipher = AesGcm::<aes::Aes256, U16>::new_from_slice(&*key).unwrap();
        let iv = [7; 16];
        let mut bytes = iv.to_vec();
        bytes.extend(
            cipher
                .encrypt(Nonce::<U16>::from_slice(&iv), value)
                .unwrap(),
        );
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn rejects_unsafe_or_empty_authentication_values() {
        for value in [
            "",
            "\r\nInjected: yes",
            "bad\0value",
            "with space",
            "with\t tab",
            "nonascii-µ",
            "\u{7f}",
        ] {
            let encoded = encrypt_test(value.as_bytes());
            for client in [true, false] {
                if value.is_empty() && !client {
                    continue;
                } // valid encrypted logout state
                let dir = fixture(CLIENT);
                let data = if client {
                    serde_json::json!({"clientId":encoded,"sessionId":SESSION})
                } else {
                    serde_json::json!({"clientId":CLIENT,"sessionId":encoded})
                };
                std::fs::write(dir.path().join("session.json"), data.to_string()).unwrap();
                assert!(
                    load(dir.path(), INSTANCE).is_err(),
                    "unsafe authentication value accepted"
                );
            }
        }
    }

    #[test]
    fn rejects_missing_invalid_and_ambiguous_json() {
        let dir = fixture(CLIENT);
        let path = dir.path().join("session.json");
        let array = serde_json::json!([CLIENT, SESSION]);
        std::fs::write(&path, array.to_string()).unwrap();
        assert!(
            load(dir.path(), INSTANCE).is_err(),
            "session must be a JSON object"
        );
        for data in [
            "",
            "not-json",
            "{}",
            "[]",
            "{\"clientId\":42,\"sessionId\":\"x\"}",
            "{\"clientId\":\"\",\"sessionId\":\"\"}",
            "{\"clientId\":\"a\",\"clientId\":\"b\",\"sessionId\":\"x\"}",
            "{\"clientId\":\"x\",\"sessionId\":null}",
        ] {
            std::fs::write(&path, data).unwrap();
            assert!(load(dir.path(), INSTANCE).is_err());
        }
        std::fs::remove_file(&path).unwrap();
        assert!(load(dir.path(), INSTANCE).is_err());
        assert!(!path.exists());
        std::fs::create_dir(&path).unwrap();
        assert!(load(dir.path(), INSTANCE).is_err());
    }

    #[test]
    fn rejects_fifo_without_blocking() {
        use std::os::unix::ffi::OsStrExt;
        let dir = fixture(CLIENT);
        let path = dir.path().join("session.json");
        std::fs::remove_file(&path).unwrap();
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(load(dir.path(), INSTANCE).is_err());
    }

    #[test]
    fn fails_closed_on_wrong_instance_without_mutation() {
        for client in [CLIENT, UUID] {
            let dir = fixture(client);
            let path = dir.path().join("session.json");
            let before = std::fs::read(&path).unwrap();
            assert!(load(dir.path(), "synthetic-wrong-instance").is_err());
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn rejects_malformed_or_tampered_ciphertext_with_redacted_errors() {
        use base64::Engine;
        let key = derive_key(INSTANCE);
        for value in ["", "synthetic-sensitive-value", "====", "AA=="] {
            let error = decrypt(value, &key).expect_err("malformed ciphertext rejected");
            assert!(!error.contains("synthetic-sensitive-value"));
        }
        let original = base64::engine::general_purpose::STANDARD
            .decode(SESSION)
            .unwrap();
        for index in [0, 16, original.len() - 1] {
            let mut bytes = original.clone();
            bytes[index] ^= 1;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            assert!(decrypt(&encoded, &key).is_err());
        }
        let non_utf8 = encrypt_test(&[0xff]);
        assert!(decrypt(&non_utf8, &key).is_err());
    }

    #[test]
    fn rejects_foreign_ownership() {
        use std::os::unix::ffi::OsStrExt;
        // chown is available only to root; ordinary-user rejection remains enforced in production.
        if unsafe { libc::geteuid() } != 0 {
            return;
        }
        for change_directory in [false, true] {
            let dir = fixture(CLIENT);
            let target = if change_directory {
                dir.path().to_path_buf()
            } else {
                dir.path().join("session.json")
            };
            let cpath = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::chown(cpath.as_ptr(), 65534, u32::MAX) }, 0);
            let rejected = load(dir.path(), INSTANCE).is_err();
            assert_eq!(unsafe { libc::chown(cpath.as_ptr(), 0, u32::MAX) }, 0);
            assert!(rejected, "foreign owner accepted");
        }
    }

    #[test]
    fn rejects_symlink_session_paths() {
        let dir = fixture(CLIENT);
        let alias = tempfile::tempdir().unwrap();
        let link = alias.path().join("linked");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        assert!(load(&link, INSTANCE).is_err(), "symlink directory accepted");
        let path = dir.path().join("session.json");
        let target = dir.path().join("target.json");
        std::fs::rename(&path, &target).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(load(dir.path(), INSTANCE).is_err(), "symlink file accepted");
    }

    #[test]
    fn rejects_insecure_permissions_without_repair() {
        use std::os::unix::fs::PermissionsExt;
        for (dir_mode, file_mode) in [
            (0o755, 0o600),
            (0o700, 0o644),
            (0o1700, 0o600),
            (0o700, 0o4600),
        ] {
            let dir = fixture(CLIENT);
            let path = dir.path().join("session.json");
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(dir_mode))
                .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(file_mode)).unwrap();
            assert!(
                load(dir.path(), INSTANCE).is_err(),
                "insecure metadata accepted"
            );
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                file_mode
            );
            assert_eq!(
                std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o7777,
                dir_mode
            );
        }
    }

    #[test]
    fn loads_legacy_uuid_without_mutation() {
        use std::os::unix::fs::MetadataExt;
        let dir = fixture(UUID);
        let path = dir.path().join("session.json");
        let before = std::fs::read(&path).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let credentials = load(dir.path(), INSTANCE).expect("legacy fixture loads");
        assert_eq!(credentials.client_id.as_str(), UUID);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let after = std::fs::metadata(path).unwrap();
        assert_eq!(
            (metadata.mode(), metadata.mtime(), metadata.mtime_nsec()),
            (after.mode(), after.mtime(), after.mtime_nsec())
        );
    }

    #[test]
    fn loads_encrypted_file_session() {
        let dir = fixture(CLIENT);
        let credentials = load(dir.path(), INSTANCE).expect("synthetic fixture loads");
        assert_eq!(credentials.client_id.as_str(), UUID);
        assert_eq!(credentials.session_id.as_str(), "synthetic-session-token");
    }

    #[test]
    fn decrypts_python_aes_gcm_iv16_fixture() {
        let key = derive_key(INSTANCE);
        assert_eq!(
            decrypt(SESSION, &key).unwrap().as_str(),
            "synthetic-session-token"
        );
        assert_eq!(decrypt(CLIENT, &key).unwrap().as_str(), UUID);
    }

    #[test]
    fn derives_python_pbkdf2_key() {
        assert_eq!(
            *derive_key(INSTANCE),
            [
                17, 98, 44, 80, 245, 137, 10, 35, 184, 65, 17, 227, 227, 74, 119, 184, 45, 154, 7,
                54, 25, 178, 201, 139, 156, 240, 225, 123, 82, 18, 66, 152
            ]
        );
    }
}
