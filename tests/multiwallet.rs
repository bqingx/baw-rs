//! Offline synthetic CLI regressions. Never use a real wallet or remote API.
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

fn cli_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baw-rs"));
    command
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID")
        .env_remove("BAW_RS_REGISTRY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.args(args).output().unwrap()
}

fn cli(args: &[&str]) -> std::process::Output {
    cli_env(args, &[])
}

fn body(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "not one JSON envelope: {} / {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn message(out: &std::process::Output) -> String {
    body(out)["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

struct Daemon(std::process::Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts the named daemon and waits for its socket. Any early exit fails the test.
fn serve(registry: &Path) -> Daemon {
    use std::time::{Duration, Instant};
    let mut child = Daemon(
        Command::new(env!("CARGO_BIN_EXE_baw-rs"))
            .env_remove("BAW_RS_REGISTRY")
            .args(["--registry", registry.to_str().unwrap(), "wallets", "serve"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let socket = registry.join("daemon.sock");
    let start = Instant::now();
    while !socket.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "daemon did not start"
        );
        assert!(child.0.try_wait().unwrap().is_none(), "daemon exited");
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}

/// Creates a wallet through the management namespace and returns its directory.
fn add_wallet(registry: &Path, root: &Path, name: &str) -> std::path::PathBuf {
    let out = cli(&[
        "--registry",
        registry.to_str().unwrap(),
        "wallets",
        "add",
        name,
        "--wallet-root",
        root.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "add {name} failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    root.join(name).join("wallet")
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
    // Legacy management shape: `vaults` stays a hidden alias for `wallets`.
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
    let mut child = serve(&r);
    let socket = r.join("daemon.sock");
    let _idle = UnixStream::connect(&socket).unwrap();
    let status = cli(&["wallets", "--registry", r.to_str().unwrap(), "status"]);
    assert!(status.status.success());
    for body in [("not-json\n".to_owned()),serde_json::json!({"op":"run","name":"missing","argv":["wallet","status"]}).to_string()+"\n",serde_json::json!({"op":"run","name":"alice","argv":["--wallet-dir=/elsewhere","wallet","status"]}).to_string()+"\n",serde_json::json!({"op":"run","name":"alice","argv":["--wallet","alice","wallet","status"]}).to_string()+"\n",serde_json::json!({"op":"run","name":"alice","argv":["tracker","ws","--following"]}).to_string()+"\n", "x".repeat(65537)+"\n"] {
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

fn empty_session_with(dir: &std::path::Path, instance: &str) {
    use aes_gcm::{
        aead::{consts::U16, Aead, KeyInit},
        AesGcm, Nonce,
    };
    use base64::Engine;
    let key = baw_rs::session::derive_key(instance);
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

fn empty_session(dir: &std::path::Path) {
    empty_session_with(dir, "legacy-synthetic");
}

/// A `wallets add` identity lives next to the wallet directory.
fn identity_of(wallet_dir: &Path) -> String {
    let path = wallet_dir.parent().unwrap().join("identity.key");
    std::fs::read_to_string(path).unwrap().trim().to_owned()
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
    assert_eq!(body(&out)["data"]["status"], "UNCONNECTED");
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
    assert_eq!(body(&out)["data"]["status"], "UNCONNECTED");
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
        "wallets",
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
        "wallets",
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
    let out = cli(&["wallets", "--registry", r, "list"]);
    assert!(out.status.success());
    assert_eq!(body(&out)["data"]["wallets"][0]["name"], "alice");
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&wallet, &alias).unwrap();
    assert!(!cli(&[
        "wallets",
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

/// The named path forwards the business command only: `--wallet` and `--registry` are
/// stripped before the IPC frame, and the frame still names the wallet.
#[test]
fn wallet_flag_is_stripped_from_the_forwarded_argv() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    empty_session(&dir);
    let listener = UnixListener::bind(registry.join("daemon.sock")).unwrap();
    let recorded = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        stream
            .write_all(format!("{}\n", json!({"success":true,"data":{"ok":true}})).as_bytes())
            .unwrap();
        line
    });
    let reg = registry.to_str().unwrap();
    let out = cli(&[
        "--registry",
        reg,
        "--wallet",
        "alice",
        "--json",
        "wallet",
        "status",
    ]);
    let line = recorded.join().unwrap();
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["op"], "run");
    assert_eq!(request["name"], "alice");
    assert_eq!(request["argv"], json!(["--json", "wallet", "status"]));
    assert!(
        !line.contains("--wallet") && !line.contains("--registry") && !line.contains(reg),
        "selectors leaked into the forwarded argv: {line}"
    );
    assert!(out.status.success(), "{}", message(&out));
}

/// `auth signin` must reach the daemon even before a session exists, and it is still
/// wallet-scoped by the same global flag.
#[test]
fn named_auth_signin_works_before_a_session_exists() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    assert!(!dir.join("session.json").exists());
    let listener = UnixListener::bind(registry.join("daemon.sock")).unwrap();
    let recorded = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        stream
            .write_all(format!("{}\n", json!({"success":true,"data":{"qrCodeId":"q"}})).as_bytes())
            .unwrap();
        line
    });
    let out = cli(&[
        "--registry",
        registry.to_str().unwrap(),
        "--wallet",
        "alice",
        "--confirm",
        "--operation-id",
        "alice-signin-001",
        "auth",
        "signin",
    ]);
    let request: Value = serde_json::from_str(&recorded.join().unwrap()).unwrap();
    assert_eq!(request["op"], "run");
    assert_eq!(request["name"], "alice");
    assert_eq!(
        request["argv"],
        json!([
            "--confirm",
            "--operation-id",
            "alice-signin-001",
            "auth",
            "signin"
        ])
    );
    assert!(out.status.success(), "{}", message(&out));
}

/// `task` is wallet-scoped at the top level and travels as native control, not as argv.
#[test]
fn named_task_routes_native_control() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    empty_session(&dir);
    let listener = UnixListener::bind(registry.join("daemon.sock")).unwrap();
    let recorded = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        stream
            .write_all(format!("{}\n", json!({"success":true,"data":{"armed":true}})).as_bytes())
            .unwrap();
        line
    });
    let out = cli(&[
        "--registry",
        registry.to_str().unwrap(),
        "--wallet",
        "alice",
        "task",
        "arm",
        "sell-1",
        "--confirm",
    ]);
    let request: Value = serde_json::from_str(&recorded.join().unwrap()).unwrap();
    assert_eq!(request["op"], "native");
    assert_eq!(request["name"], "alice");
    assert_eq!(
        request["command"],
        json!({"action":"arm","task_id":"sell-1","confirm":true})
    );
    assert!(out.status.success(), "{}", message(&out));
}

/// No wallet and no legacy pair is an error that names the missing option.
#[test]
fn missing_wallet_flag_names_the_option() {
    for args in [
        vec!["wallet", "status"],
        vec!["wallet", "address"],
        vec!["auth", "signin", "--confirm", "--operation-id", "x"],
        vec!["task", "status"],
        vec!["approvals", "list"],
    ] {
        let out = cli(&args);
        assert!(!out.status.success(), "{args:?} should fail");
        let text = message(&out);
        assert!(
            text.contains("--wallet <NAME>"),
            "{args:?} message did not name --wallet: {text}"
        );
        assert_eq!(text.lines().count(), 1);
    }
    // Management never needs it.
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    add_wallet(
        &root.path().join("registry"),
        &root.path().join("wallets"),
        "alice",
    );
    let out = cli(&[
        "wallets",
        "--registry",
        root.path().join("registry").to_str().unwrap(),
        "list",
    ]);
    assert!(out.status.success());
}

/// An unknown name fails before any socket work, with no fallback to another wallet.
#[test]
fn unknown_wallet_fails_before_the_socket() {
    use std::os::unix::net::UnixListener;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    empty_session(&dir);
    let listener = UnixListener::bind(registry.join("daemon.sock")).unwrap();
    let reg = registry.to_str().unwrap();
    for name in ["bob", "Alice", "ali ce"] {
        let out = cli(&["--registry", reg, "--wallet", name, "wallet", "status"]);
        assert!(!out.status.success(), "{name} unexpectedly resolved");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("unknown wallet") || text.contains("invalid wallet name"),
            "{name}: {text}"
        );
        assert!(!text.contains("alice;"), "{name} fell back to alice");
    }
    // No registry at all: still no fallback, and the error explains the missing path.
    let missing = root.path().join("absent-registry");
    let out = cli(&[
        "--registry",
        missing.to_str().unwrap(),
        "--wallet",
        "alice",
        "wallet",
        "status",
    ]);
    assert!(!out.status.success());
    assert!(message(&out).contains("does not exist"));
    listener.set_nonblocking(true).unwrap();
    assert!(
        listener.accept().is_err(),
        "unknown wallet reached the daemon socket"
    );
}

/// Two named wallets share one daemon but never share sessions, and errors name the
/// wallet that failed.
#[test]
fn two_wallets_are_isolated_through_a_real_daemon() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let root_dir = root.path().join("wallets");
    let alice = add_wallet(&registry, &root_dir, "alice");
    let bob = add_wallet(&registry, &root_dir, "bob");
    empty_session_with(&alice, &identity_of(&alice));
    assert!(!bob.join("session.json").exists());
    let mut child = serve(&registry);
    let reg = registry.to_str().unwrap();
    let status = cli(&["--registry", reg, "wallets", "status"]);
    assert!(status.status.success());
    assert_eq!(body(&status)["data"]["wallets"], json!(["alice", "bob"]));
    // alice has an (empty) session file: reads reach the daemon.
    let out = cli(&["--registry", reg, "--wallet", "alice", "wallet", "status"]);
    assert!(out.status.success(), "{}", message(&out));
    assert_eq!(body(&out)["data"]["status"], "UNCONNECTED");
    // alice is not actually signed in, and that failure names alice.
    let out = cli(&["--registry", reg, "--wallet", "alice", "wallet", "address"]);
    assert!(!out.status.success());
    assert!(message(&out).contains("'alice'"), "{}", message(&out));
    // bob never conflates with alice: he has no session at all.
    let out = cli(&["--registry", reg, "--wallet", "bob", "wallet", "status"]);
    assert!(!out.status.success());
    assert!(
        message(&out).contains("wallet 'bob' is not signed in"),
        "{}",
        message(&out)
    );
    // alice still works after bob failed.
    let out = cli(&["--registry", reg, "--wallet", "alice", "wallet", "status"]);
    assert!(out.status.success(), "{}", message(&out));
    assert!(cli(&["--registry", reg, "wallets", "stop"])
        .status
        .success());
    let start = std::time::Instant::now();
    while child.0.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// add creates private paths, remove unregisters, and data survives unless explicitly
/// deleted with consent.
#[test]
fn wallets_add_remove_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let wallets = root.path().join("wallets");
    let reg = registry.to_str().unwrap();
    let dir = add_wallet(&registry, &wallets, "alice");
    let key = wallets.join("alice").join("identity.key");
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode(&wallets), 0o700);
    assert_eq!(mode(&wallets.join("alice")), 0o700);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&key), 0o600);
    assert!(!dir.join("session.json").exists());
    let listed = cli(&["--registry", reg, "wallets", "list"]);
    assert_eq!(body(&listed)["data"]["wallets"][0]["name"], "alice");
    assert_eq!(body(&listed)["data"]["wallets"][0]["signedIn"], false);
    // Duplicate name, and re-adding over existing paths, both fail closed.
    assert!(!cli(&[
        "--registry",
        reg,
        "wallets",
        "add",
        "alice",
        "--wallet-root",
        wallets.to_str().unwrap()
    ])
    .status
    .success());
    // remove keeps data by default.
    let out = cli(&["--registry", reg, "wallets", "remove", "alice"]);
    assert!(out.status.success(), "{}", message(&out));
    assert_eq!(body(&out)["data"]["dataKept"], true);
    assert!(dir.exists() && key.exists());
    assert_eq!(
        body(&cli(&["--registry", reg, "wallets", "list"]))["data"]["wallets"],
        json!([])
    );
    assert!(!cli(&["--registry", reg, "wallets", "remove", "alice"])
        .status
        .success());
    // Re-register the kept data, then delete it explicitly.
    let out = cli(&[
        "--registry",
        reg,
        "wallets",
        "register",
        "alice",
        "--wallet-dir",
        dir.to_str().unwrap(),
        "--instance-id-file",
        key.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", message(&out));
    assert!(!cli(&[
        "--registry",
        reg,
        "wallets",
        "remove",
        "alice",
        "--delete-data"
    ])
    .status
    .success());
    assert!(dir.exists(), "data deleted without --confirm");
    let out = cli(&[
        "--registry",
        reg,
        "wallets",
        "remove",
        "alice",
        "--delete-data",
        "--confirm",
    ]);
    assert!(out.status.success(), "{}", message(&out));
    assert!(!dir.exists() && !key.exists());
}

/// Management is wallet-free, and combining it with --wallet is rejected instead of
/// silently ignoring the name.
#[test]
fn management_namespace_never_requires_a_wallet() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = root.path().join("registry");
    let wallets = root.path().join("wallets");
    let reg = registry.to_str().unwrap();
    add_wallet(&registry, &wallets, "alice");
    // --registry from the environment, not the command line.
    let out = cli_env(&["wallets", "list"], &[("BAW_RS_REGISTRY", reg)]);
    assert!(out.status.success(), "{}", message(&out));
    assert_eq!(body(&out)["data"]["wallets"][0]["name"], "alice");
    let out = cli(&["--wallet", "alice", "wallets", "list"]);
    assert!(!out.status.success());
    assert!(
        message(&out).contains("takes no --wallet"),
        "{}",
        message(&out)
    );
    // `status` needs the daemon and says so instead of blaming the wallet.
    let out = cli(&["--registry", reg, "wallets", "status"]);
    assert!(!out.status.success());
    assert!(
        message(&out).contains("resident daemon is not running"),
        "{}",
        message(&out)
    );
}

/// Defaults: $BAW_RS_REGISTRY, else $HOME/wallets/registry. Never the legacy selectors.
#[test]
fn registry_defaults_to_env_then_home() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let registry = home.join("wallets").join("registry");
    add_wallet(&registry, &home.join("wallets"), "alice");
    // HOME default, no env, no flag.
    let out = cli_env(&["wallets", "list"], &[("HOME", home.to_str().unwrap())]);
    assert!(out.status.success(), "{}", message(&out));
    assert_eq!(body(&out)["data"]["wallets"][0]["name"], "alice");
    // The named path uses the same default.
    let out = cli_env(
        &["--wallet", "alice", "wallet", "status"],
        &[("HOME", home.to_str().unwrap())],
    );
    assert!(!out.status.success());
    assert!(
        !message(&out).contains("does not exist"),
        "named path did not find the HOME registry: {}",
        message(&out)
    );
}
