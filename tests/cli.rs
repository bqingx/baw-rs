use baw_rs::cli::{parse_batch_line, Cli, Command, WalletCommand};
use clap::Parser;
use std::io::Write;
use std::process::{Command as Process, Stdio};

fn binary() -> Process {
    let mut command = Process::new(env!("CARGO_BIN_EXE_baw-rs"));
    command
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID");
    command
}

#[test]
fn executable_errors_are_single_sanitized_json_without_session_access() {
    for args in [
        vec!["wallet", "status"],
        vec![
            "--wallet-dir",
            "/synthetic-does-not-exist",
            "--instance-id",
            "synthetic-instance",
            "wallet",
            "status",
        ],
        vec![
            "--wallet-dir",
            "/synthetic",
            "--instance-id",
            "x",
            "wallet",
            "send",
            "secret-argument",
        ],
    ] {
        let output = binary().args(args).output().unwrap();
        assert!(!output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(text.lines().count(), 1);
        let row: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(row["success"], false);
        assert!(row["error"]["message"].is_string());
        assert!(!text.contains("secret-argument"));
    }
}

fn synthetic_wallet() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let content = r#"{"clientId":"01234567-89ab-4cde-8fab-0123456789ab","sessionId":"AAECAwQFBgcICQoLDA0OD5bjeqbdyeBoraR8XA6uMlE0+39LF7Kj+vOLW2/aVI4LHlNiZ6em1Q=="}"#;
    std::fs::write(dir.path().join("session.json"), content).unwrap();
    std::fs::set_permissions(
        dir.path().join("session.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    dir
}

#[test]
fn executable_batch_rejects_invalid_input_without_writing_synthetic_wallet() {
    let dir = synthetic_wallet();
    let before = std::fs::read(dir.path().join("session.json")).unwrap();
    let mut child = binary()
        .arg("--wallet-dir")
        .arg(dir.path())
        .args([
            "--instance-id",
            "synthetic-baw-test-instance",
            "--timing",
            "batch",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"[\"batch\"]\n[\"wallet\",\"gas-price\"]\n[\"--wallet-dir=/other\",\"wallet\",\"status\"]\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["success"] == false));
    assert_eq!(String::from_utf8(output.stderr).unwrap().lines().count(), 3);
    assert_eq!(
        before,
        std::fs::read(dir.path().join("session.json")).unwrap()
    );
}

#[test]
fn executable_batch_exits_cleanly_on_broken_pipe() {
    let dir = synthetic_wallet();
    let mut child = binary()
        .arg("--wallet-dir")
        .arg(dir.path())
        .args(["--instance-id", "synthetic-baw-test-instance", "batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"[\"batch\"]\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}

#[test]
fn explicit_wallet_selection_and_read_only_allowlist() {
    let args = [
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "synthetic-instance",
        "--json",
        "wallet",
        "status",
    ];
    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Command::Wallet {
            command: WalletCommand::Status
        }
    ));
    for command in ["send", "swap", "login", "execute", "arbitrary-endpoint"] {
        assert!(Cli::try_parse_from([
            "baw-rs",
            "--wallet-dir",
            "/synthetic",
            "--instance-id",
            "x",
            "wallet",
            command
        ])
        .is_err());
    }
    assert!(Cli::try_parse_from([
        "baw-rs",
        "--wallet-dir",
        "/synthetic",
        "--instance-id",
        "x",
        "wallet",
        "gas-price"
    ])
    .is_err());
}

#[test]
fn batch_rejects_overrides_nested_batch_and_unbounded_inputs() {
    assert!(parse_batch_line(r#"["wallet","status"]"#).is_ok());
    assert!(parse_batch_line(r#"["wallet","balance"]"#).is_ok());
    assert!(parse_batch_line(r#"["wallet","balance","--raw"]"#).is_ok());
    for input in [
        r#"["batch"]"#,
        r#"["--wallet-dir","/other","wallet","status"]"#,
        r#"["wallet","status","--instance-id=x"]"#,
        r#"["wallet","status","--json"]"#,
        r#"["wallet","gas-price"]"#,
        "{}",
        "[1]",
        "null",
    ] {
        assert!(parse_batch_line(input).is_err(), "accepted {input}");
    }
    assert!(parse_batch_line(&" ".repeat(65537)).is_err());
}
