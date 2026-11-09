use serde_json::Value;
use std::process::Command;

#[test]
fn intent_runs_before_wallet_session_registry_and_private_config_access() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("must-not-be-created");
    for named in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_baw-rs"));
        cmd.env_remove("BINANCE_BAW_DIR")
            .env_remove("BINANCE_INSTANCE_ID")
            .env_remove("BAW_RS_SKIP_PRECHECKS");
        if named {
            cmd.args(["--wallet", "not-registered", "--registry"])
                .arg(&missing)
                .arg("--warm");
        } else {
            cmd.arg("--wallet-dir")
                .arg(&missing)
                .arg("--instance-id-file")
                .arg(missing.join("identity"));
        }
        let output = cmd
            .args([
                "market-order",
                "intent",
                "--binanceChainId",
                "56",
                "--fromToken",
                "0x1111111111111111111111111111111111111111",
                "--toToken",
                "0x2222222222222222222222222222222222222222",
                "--fromTokenQty",
                "1.25",
                "--slippage",
                "0.5",
                "--mev",
                "false",
                "--confirm",
                "--operation-id",
                "must-not-write",
                "--okx-config",
            ])
            .arg(missing.join("private-config"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["data"]["executionAuthorized"], false);
        assert_eq!(v["data"]["noDeadlineGranted"], true);
        assert_eq!(
            v["data"]["readiness"]["missing"],
            serde_json::json!(["fromTokenDecimals", "acknowledgeTokenRisk"])
        );
        assert_eq!(v["data"]["minimumOutputPolicy"]["mode"], "fresh_route");
        assert!(v["data"]["output"]["userMinimumUnits"].is_null());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private-config"));
        assert!(!missing.exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn intent_needs_no_wallet_and_does_not_weaken_swap_gate() {
    let args = [
        "market-order",
        "intent",
        "--binanceChainId",
        "56",
        "--fromToken",
        "0x1111111111111111111111111111111111111111",
        "--toToken",
        "0x2222222222222222222222222222222222222222",
        "--fromTokenQty",
        "1.25",
        "--slippage",
        "0.5",
        "--mev",
        "false",
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut swap = args;
    swap[1] = "swap";
    let output = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BINANCE_BAW_DIR")
        .env_remove("BINANCE_INSTANCE_ID")
        .args(swap)
        .output()
        .unwrap();
    assert!(!output.status.success());
}
