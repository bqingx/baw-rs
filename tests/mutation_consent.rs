//! Consent gates run inside the resident daemon before any wallet work: a signed-out
//! wallet still gets "requires ..." instead of a session error, and nothing is sent.
mod common;
use common::{add_wallet, body, cli, scratch, serve, sign_out};

#[test]
fn unauthorized_writes_fail_before_wallet_access() {
    let root = scratch();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    sign_out(&dir);
    let daemon = serve(&registry);
    let reg = registry.to_str().unwrap();
    let cases = [
        vec!["auth", "signin"],
        vec!["auth", "verify", "--qrCodeId", "synthetic"],
        vec!["auth", "signout"],
        vec![
            "wallet",
            "send",
            "--binanceChainId",
            "56",
            "--tokenAddress",
            "0x0000000000000000000000000000000000000001",
            "--recipient",
            "0x0000000000000000000000000000000000000002",
            "--amount",
            "0.000000000000000001",
        ],
        vec![
            "market-order",
            "swap",
            "--binanceChainId",
            "56",
            "--fromToken",
            "0x0000000000000000000000000000000000000001",
            "--toToken",
            "0x0000000000000000000000000000000000000002",
            "--fromTokenQty",
            "1",
            "--slippage",
            "0.5",
            "--mev",
            "false",
            "--okx-config",
            "/synthetic/profile.json",
            "--fromTokenDecimals",
            "18",
            "--acknowledge-token-risk",
            "--okx-min-output-units",
            "1",
        ],
        vec!["contract-call", "execute", "--requestId", "synthetic"],
        vec!["sign-message", "execute", "--requestId", "synthetic"],
        // Remaining parity family with a writing leaf.
        vec![
            "approvals",
            "revoke",
            "--binanceChainId",
            "56",
            "--tokenContract",
            "0x0000000000000000000000000000000000000001",
            "--spender",
            "0x0000000000000000000000000000000000000002",
            "--type",
            "approve",
        ],
        vec![
            "wallet",
            "speed-up",
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        ],
    ];
    for case in cases {
        for consent in [vec![], vec!["--confirm"], vec!["--operation-id", "only-id"]] {
            let mut args = vec!["--registry", reg, "--wallet", "alice"];
            args.extend(consent);
            args.extend(case.clone());
            let out = cli(&args);
            assert!(!out.status.success(), "{case:?} unexpectedly succeeded");
            let row = body(&out);
            assert!(
                row["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("requires"),
                "{case:?}: {row}"
            );
            assert!(out.stderr.is_empty());
        }
    }
    daemon.stop(&registry);
    // No journal entry may exist: the gate rejected the command before the guard opened.
    assert!(!dir.join(".baw-rs-operations").exists());
}
#[test]
fn unreviewed_signing_fails_before_session_access() {
    let root = scratch();
    let registry = root.path().join("registry");
    let dir = add_wallet(&registry, &root.path().join("wallets"), "alice");
    sign_out(&dir);
    let daemon = serve(&registry);
    let reg = registry.to_str().unwrap();
    for (args, expected) in [
        (
            vec![
                "--confirm",
                "--operation-id",
                "unreviewed",
                "contract-call",
                "execute",
                "--requestId",
                "synthetic",
            ],
            "requires --accept-preview-sha256",
        ),
        (
            vec![
                "--confirm",
                "--operation-id",
                "unreviewed",
                "sign-message",
                "execute",
                "--requestId",
                "synthetic",
            ],
            "requires --accept-preview-sha256",
        ),
    ] {
        let mut full = vec!["--registry", reg, "--wallet", "alice"];
        full.extend(args);
        let output = cli(&full);
        assert!(!output.status.success());
        let row = body(&output);
        assert!(
            row["error"]["message"].as_str().unwrap().contains(expected),
            "{row}"
        );
    }
    daemon.stop(&registry);
}
