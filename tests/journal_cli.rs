//! `wallets journal` is a local read of one wallet's operation journal: no daemon, no
//! session, no identity and no network are involved, so reconciliation works even when
//! the wallet is signed out or the resident daemon is stopped.
mod common;
use common::{add_wallet, body, cli, journal_entry, message, scratch};

#[test]
fn journal_reads_records_without_a_daemon_or_a_session() {
    let root = scratch();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    journal_entry(&dir, "op-1", r#"{"state":"SUBMITTING"}"#);
    journal_entry(
        &dir,
        "op-2",
        r#"{"state":"RESPONSE_RECEIVED","recovery":{"txHash":"0xabc"}}"#,
    );
    assert!(!dir.join("session.json").exists());
    let reg = registry.to_str().unwrap();
    // The daemon was never started: there is no socket to talk to.
    assert!(!registry.join("daemon.sock").exists());

    let listed = cli(&["--registry", reg, "wallets", "journal", "alice", "--list"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stdout)
    );
    let listed = body(&listed);
    assert_eq!(
        listed["data"]["operations"],
        serde_json::json!(["op-1", "op-2"])
    );
    assert_eq!(listed["data"]["count"], 2);

    // Single record through the global --operation-id, before or after the subcommand.
    for args in [
        vec![
            "--registry",
            reg,
            "wallets",
            "journal",
            "alice",
            "--operation-id",
            "op-1",
        ],
        vec![
            "--registry",
            reg,
            "--operation-id",
            "op-2",
            "wallets",
            "journal",
            "alice",
        ],
    ] {
        let out = cli(&args);
        assert!(out.status.success(), "{}", message(&out));
        let row = body(&out);
        assert!(row["data"]["verdict"].is_string(), "{row}");
    }
    // A journal entry that only proves "maybe submitted" never claims a retry is safe.
    let record = body(&cli(&[
        "--registry",
        reg,
        "wallets",
        "journal",
        "alice",
        "--operation-id",
        "op-1",
    ]));
    assert_eq!(record["data"]["verdict"], "UNCERTAIN");
    assert_eq!(record["data"]["retrySafe"], false);
    // Purely local: no session, no socket, and nothing written to the wallet directory.
    assert!(!dir.join("session.json").exists());
    assert!(!registry.join("daemon.sock").exists());
    assert!(!dir.join("auth-pending").exists());
}

#[test]
fn journal_requires_one_selector_and_a_registered_name() {
    let root = scratch();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    journal_entry(&dir, "op-1", r#"{"state":"SUBMITTING"}"#);
    let reg = registry.to_str().unwrap();

    let neither = cli(&["--registry", reg, "wallets", "journal", "alice"]);
    assert!(!neither.status.success());
    assert!(
        message(&neither).contains("--operation-id <ID> or --list"),
        "{}",
        message(&neither)
    );

    let both = cli(&[
        "--registry",
        reg,
        "wallets",
        "journal",
        "alice",
        "--list",
        "--operation-id",
        "op-1",
    ]);
    assert!(!both.status.success());
    assert!(message(&both).contains("not both"), "{}", message(&both));

    for name in ["ghost", "ali ce"] {
        let out = cli(&["--registry", reg, "wallets", "journal", name, "--list"]);
        assert!(!out.status.success(), "{name} unexpectedly resolved");
        let text = message(&out);
        assert!(
            text.contains("unknown wallet") || text.contains("invalid wallet name"),
            "{name}: {text}"
        );
    }
    // A missing record is an error, never an empty success.
    let absent = cli(&[
        "--registry",
        reg,
        "wallets",
        "journal",
        "alice",
        "--operation-id",
        "absent",
    ]);
    assert!(!absent.status.success());
    assert!(
        message(&absent).contains("no operation journal entry"),
        "{}",
        message(&absent)
    );
}
