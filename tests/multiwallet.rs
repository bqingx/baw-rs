//! Offline synthetic CLI regressions. Never use a real wallet or remote API.
use std::{os::unix::fs::PermissionsExt, process::Command};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID")
        .args(args)
        .output()
        .unwrap()
}
struct Daemon(std::process::Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn daemon_lifecycle_bounded_ipc_and_no_selector_fallback() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixStream,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let r = root.path().join("r");
    let w = root.path().join("w");
    let k = root.path().join("key");
    std::fs::create_dir(&w).unwrap();
    std::fs::set_permissions(&w, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(&k, "synthetic-b").unwrap();
    std::fs::set_permissions(&k, std::fs::Permissions::from_mode(0o600)).unwrap();
    let out = cli(&[
        "vaults",
        "--registry",
        r.to_str().unwrap(),
        "register",
        "alice",
        "--wallet-dir",
        w.to_str().unwrap(),
        "--instance-id-file",
        k.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let mut child = Daemon(
        Command::new(env!("CARGO_BIN_EXE_baw-rs"))
            .args(["vaults", "--registry", r.to_str().unwrap(), "serve"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let socket = r.join("daemon.sock");
    let start = Instant::now();
    while !socket.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "daemon did not start"
        );
        assert!(child.0.try_wait().unwrap().is_none(), "daemon exited");
        std::thread::sleep(Duration::from_millis(10));
    }
    let _idle = UnixStream::connect(&socket).unwrap();
    let status = cli(&["vaults", "--registry", r.to_str().unwrap(), "status"]);
    assert!(status.status.success());
    for body in ["not-json\n".to_owned(),serde_json::json!({"op":"run","name":"missing","argv":["wallet","status"]}).to_string()+"\n",serde_json::json!({"op":"run","name":"alice","argv":["--wallet-dir=/elsewhere","wallet","status"]}).to_string()+"\n",serde_json::json!({"op":"run","name":"alice","argv":["tracker","ws","--following"]}).to_string()+"\n", "x".repeat(65537)+"\n"] {
        let mut s=UnixStream::connect(&socket).unwrap(); s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let _=s.write_all(body.as_bytes()); let mut line=String::new(); BufReader::new(s).read_line(&mut line).unwrap(); let v:serde_json::Value=serde_json::from_str(&line).unwrap(); assert_eq!(v["success"],false);
    }
    assert!(cli(&["vaults", "--registry", r.to_str().unwrap(), "stop"])
        .status
        .success());
    let start = Instant::now();
    while child.0.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!socket.exists());
    assert!(!w.join("session.json").exists());
}
fn empty_session(dir: &std::path::Path) {
    use aes_gcm::{
        aead::{consts::U16, Aead, KeyInit},
        AesGcm, Nonce,
    };
    use base64::Engine;
    let key = baw_rs::session::derive_key("legacy-synthetic");
    let cipher = AesGcm::<aes::Aes256, U16>::new_from_slice(&*key).unwrap();
    let iv = [17u8; 16];
    let mut bytes = iv.to_vec();
    bytes.extend(
        cipher
            .encrypt(Nonce::<U16>::from_slice(&iv), b"".as_slice())
            .unwrap(),
    );
    std::fs::write(dir.join("session.json"),serde_json::json!({"clientId":"11111111-1111-4111-8111-111111111111","sessionId":base64::engine::general_purpose::STANDARD.encode(bytes)}).to_string()).unwrap();
    std::fs::set_permissions(
        dir.join("session.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}
#[test]
fn legacy_single_wallet_cold_and_warm_signed_out_compatibility() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    empty_session(root.path());
    let dir = root.path().to_str().unwrap();
    let args = ["--wallet-dir", dir, "--instance-id", "legacy-synthetic"];
    let mut cold = args.to_vec();
    cold.extend(["wallet", "status"]);
    let out = cli(&cold);
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["data"]["status"],
        "UNCONNECTED"
    );
    let mut child = Daemon(
        Command::new(env!("CARGO_BIN_EXE_baw-rs"))
            .args(args)
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = std::time::Instant::now();
    while !root.path().join(".baw-rs.sock").exists() {
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "legacy warm rejected signed-out wallet"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut warm = args.to_vec();
    warm.extend(["--warm", "wallet", "status"]);
    let out = cli(&warm);
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["data"]["status"],
        "UNCONNECTED"
    );
}
#[test]
fn malformed_registry_never_becomes_an_empty_registry() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(
        root.path().join("absent"),
        root.path().join("registry.json"),
    )
    .unwrap();
    let out = cli(&[
        "vaults",
        "--registry",
        root.path().to_str().unwrap(),
        "list",
    ]);
    assert!(
        !out.status.success(),
        "broken registry symlink silently treated as empty registry"
    );
}
#[test]
fn registry_register_list_duplicate_and_unknown_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("registry");
    let wallet = root.path().join("wallet");
    std::fs::create_dir(&wallet).unwrap();
    std::fs::set_permissions(&wallet, std::fs::Permissions::from_mode(0o700)).unwrap();
    let key = root.path().join("identity");
    std::fs::write(&key, "synthetic-instance-a\n").unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let r = registry.to_str().unwrap();
    let w = wallet.to_str().unwrap();
    let k = key.to_str().unwrap();
    let out = cli(&[
        "vaults",
        "--registry",
        r,
        "register",
        "alice",
        "--wallet-dir",
        w,
        "--instance-id-file",
        k,
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = cli(&["vaults", "--registry", r, "list"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["wallets"][0]["name"], "alice");
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&wallet, &alias).unwrap();
    assert!(!cli(&[
        "vaults",
        "--registry",
        r,
        "register",
        "bob",
        "--wallet-dir",
        alias.to_str().unwrap(),
        "--instance-id-file",
        k
    ])
    .status
    .success());
    assert!(!cli(&[
        "vaults",
        "--registry",
        r,
        "run",
        "missing",
        "--",
        "wallet",
        "status"
    ])
    .status
    .success());
    assert!(!wallet.join("session.json").exists());
}
