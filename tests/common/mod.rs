//! Shared synthetic fixtures for the named-wallet path: a temporary registry, wallets
//! created by `wallets add`, and a daemon on a temporary socket. Nothing here reads or
//! writes a real wallet directory, a real registry, or the network.
#![allow(dead_code)]
use serde_json::Value;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

/// Run the binary with the ambient wallet-related state removed.
pub fn cli<S: AsRef<OsStr>>(args: &[S]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BAW_RS_REGISTRY")
        .env_remove("BAW_RS_NOTIFY_CONFIG")
        .env_remove("BAW_RS_SKIP_PRECHECKS")
        .args(args)
        .output()
        .unwrap()
}

pub fn body(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "not one JSON envelope: {} / {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

pub fn message(out: &Output) -> String {
    body(out)["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// A 0700 scratch root; the test owns every path below it.
pub fn scratch() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}

/// Register a wallet through the management namespace and return its directory.
pub fn add_wallet(registry: &Path, wallet_root: &Path, name: &str) -> PathBuf {
    let out = cli(&[
        "--registry",
        registry.to_str().unwrap(),
        "wallets",
        "add",
        name,
        "--wallet-root",
        wallet_root.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "add {name} failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    wallet_root.join(name).join("wallet")
}

/// The identity `wallets add` wrote next to the wallet directory.
pub fn identity_of(wallet_dir: &Path) -> String {
    let path = wallet_dir.parent().unwrap().join("identity.key");
    std::fs::read_to_string(path).unwrap().trim().to_owned()
}

/// Write a decryptable but empty session, so the wallet is "signed out" rather than
/// "no session file": requests then reach the daemon without any usable credential.
pub fn sign_out(wallet_dir: &Path) {
    use aes_gcm::{
        aead::{consts::U16, Aead, KeyInit},
        AesGcm, Nonce,
    };
    use base64::Engine;
    let key = baw_rs::session::derive_key(&identity_of(wallet_dir));
    let cipher = AesGcm::<aes::Aes256, U16>::new_from_slice(&*key).unwrap();
    let iv = [17u8; 16];
    let mut bytes = iv.to_vec();
    bytes.extend(
        cipher
            .encrypt(Nonce::<U16>::from_slice(&iv), b"".as_slice())
            .unwrap(),
    );
    let path = wallet_dir.join("session.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "clientId":"11111111-1111-4111-8111-111111111111",
            "sessionId": base64::engine::general_purpose::STANDARD.encode(bytes),
        })
        .to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

/// A private journal entry exactly as `OperationGuard` would leave one behind.
pub fn journal_entry(wallet_dir: &Path, id: &str, body: &str) {
    let journal = wallet_dir.join(".baw-rs-operations");
    std::fs::create_dir_all(&journal).unwrap();
    std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = journal.join(format!("{id}.json"));
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

pub struct Daemon(Child);

impl Daemon {
    /// Ask for a graceful stop and wait for exit.
    pub fn stop(mut self, registry: &Path) {
        let out = cli(&["--registry", registry.to_str().unwrap(), "wallets", "stop"]);
        assert!(
            out.status.success(),
            "stop failed: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let start = std::time::Instant::now();
        while self.0.try_wait().unwrap().is_none() {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "daemon did not exit"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start the named daemon and wait for its socket.
pub fn serve(registry: &Path) -> Daemon {
    use std::time::{Duration, Instant};
    let mut child = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BAW_RS_REGISTRY")
        .env_remove("BAW_RS_NOTIFY_CONFIG")
        .args(["--registry", registry.to_str().unwrap(), "wallets", "serve"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !registry.join("daemon.sock").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "daemon did not start"
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "daemon exited before its socket appeared"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Daemon(child)
}
