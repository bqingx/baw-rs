//! Pure local description and IPC snapshot, with no host credentials or network.
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};
fn command(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .env_remove("BAW_RS_REGISTRY")
        .env_remove("BAW_RS_NOTIFY_CONFIG")
        .env_remove("BAW_RS_SKIP_PRECHECKS")
        .args(args)
        .output()
        .unwrap()
}
fn private(path: &Path, value: &Value) {
    std::fs::write(path, value.to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}
#[test]
fn pipeline_description_is_local_does_not_load_arm_or_read_private_policy() {
    let root = root();
    let config = root.path().join("task.json");
    let registry = root.path().join("not-created");
    private(
        &config,
        &json!({"id":"intent-demo","chain_id":"56","steps":[{"task_id":"step1","trigger":{"kind":"immediate"},"completion":"submitted","valid_for_ms":60000,"actions":[{"wallet":"w1","argv":["contract-call","fire","--binanceChainId","56","--from","0x1111111111111111111111111111111111111111","--to","0x2222222222222222222222222222222222222222","--value","9007199254740993123456789","--inputData","0xdeadbeef","--policy","/does-not-exist/private-secret-policy"]}]}]}),
    );
    let before = std::fs::read(&config).unwrap();
    let result = command(&[
        "--registry",
        registry.to_str().unwrap(),
        "pipeline",
        "describe",
        "--config",
        config.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    let data = &result["data"];
    assert_eq!(data["executionAuthorized"], false);
    assert_eq!(data["deadlineGranted"], false);
    assert_eq!(data["loaded"], false);
    assert_eq!(data["networkRequests"], 0);
    assert_eq!(
        data["steps"][0]["actions"][0]["intent"]["valueUnits"],
        "9007199254740993123456789"
    );
    assert!(!registry.exists());
    assert_eq!(std::fs::read(&config).unwrap(), before);
    assert!(!result.to_string().contains("private-secret-policy"));
    assert!(!result.to_string().contains("deadbeef"));
}
#[test]
fn contract_description_rejects_invalid_calldata_and_gas_before_claiming_valid_definition() {
    let root = root();
    let config = root.path().join("task.json");
    let registry = root.path().join("not-created");
    for extra in [vec!["--inputData", "0x1"], vec!["--gasLimit", "1"]] {
        let mut argv = vec![
            "contract-call",
            "fire",
            "--binanceChainId",
            "56",
            "--from",
            "0x1111111111111111111111111111111111111111",
            "--to",
            "0x2222222222222222222222222222222222222222",
            "--value",
            "0",
        ];
        argv.extend(extra);
        private(
            &config,
            &json!({"id":"invalid-intent","chain_id":"56","steps":[{"task_id":"step1","trigger":{"kind":"immediate"},"completion":"submitted","valid_for_ms":60000,"actions":[{"wallet":"w1","argv":argv}]}]}),
        );
        let output = command(&[
            "--registry",
            registry.to_str().unwrap(),
            "pipeline",
            "describe",
            "--config",
            config.to_str().unwrap(),
        ]);
        assert!(!output.status.success());
        assert!(!registry.exists());
    }
}

#[test]
fn swap_task_description_does_not_open_profile_or_grant_listening_window() {
    let root = root();
    let config = root.path().join("task.json");
    let registry = root.path().join("not-created");
    private(
        &config,
        &json!({"id":"swap-demo","chain_id":"56","steps":[{"task_id":"swap","trigger":{"kind":"immediate"},"completion":"submitted","valid_for_ms":300000,"actions":[{"wallet":"w1","argv":["market-order","swap","--okx-config","/does-not-exist/secret-profile","--binanceChainId","56","--fromToken","0x1111111111111111111111111111111111111111","--toToken","0x2222222222222222222222222222222222222222","--fromTokenQty","1.25","--fromTokenDecimals","18","--slippage","1","--mev","false","--okx-min-output-units","1","--acknowledge-token-risk","--auto-approve"]}]}]}),
    );
    let result = command(&[
        "--registry",
        registry.to_str().unwrap(),
        "pipeline",
        "describe",
        "--config",
        config.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["data"]["steps"][0]["validForMs"], 300000);
    assert_eq!(value["data"]["steps"][0]["deadlineGranted"], false);
    assert_eq!(
        value["data"]["steps"][0]["actions"][0]["intent"]["input"]["units"],
        "1250000000000000000"
    );
    assert_eq!(value["data"]["executionAuthorized"], false);
    assert!(!value.to_string().contains("secret-profile"));
    let mut definition: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    let argv = definition["steps"][0]["actions"][0]["argv"]
        .as_array_mut()
        .unwrap();
    let index = argv
        .iter()
        .position(|v| v == "--okx-min-output-units")
        .unwrap();
    argv.drain(index..index + 2);
    private(&config, &definition);
    let output = command(&[
        "--registry",
        registry.to_str().unwrap(),
        "pipeline",
        "describe",
        "--config",
        config.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let intent = &value["data"]["steps"][0]["actions"][0]["intent"];
    assert_eq!(intent["minimumOutputPolicy"]["mode"], "fresh_route");
    assert!(intent["output"]["userMinimumUnits"].is_null());
    assert_eq!(value["data"]["deadlineGranted"], false);
    assert!(!registry.exists());
}
