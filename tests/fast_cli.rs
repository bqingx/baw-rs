use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn build(value: Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .args(["fast", "build"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{value}").unwrap();
    child.wait_with_output().unwrap()
}
fn buy() -> Value {
    json!({"core":"0x2222222222222222222222222222222222222222","token":"0x3333333333333333333333333333333333333333","from":"0x1111111111111111111111111111111111111111","budget":"340282366920938463463374607431768211456","minAmountOut":"0","allowZeroMinOut":true})
}
#[test]
fn explicit_offline_parameters_match_json_encoder_without_wallet() {
    let rpc = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    rpc.set_nonblocking(true).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_baw-rs"))
        .args([
            "openfour",
            "encode-buy",
            "--core",
            buy()["core"].as_str().unwrap(),
            "--token",
            buy()["token"].as_str().unwrap(),
            "--wallet",
            buy()["from"].as_str().unwrap(),
            "--amount-wei",
            buy()["budget"].as_str().unwrap(),
            "--min-out-raw",
            "0",
            "--allow-zero-min-out",
        ])
        .env(
            "BSC_RPC_URL",
            format!("http://{}", rpc.local_addr().unwrap()),
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let other: Value = serde_json::from_slice(&build(buy()).stdout).unwrap();
    assert_eq!(v, other);
    assert_eq!(
        rpc.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "offline encoder must make zero RPC requests"
    );
}
#[test]
fn matches_independent_eth_abi_fixtures() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/openfour_budget.json")).unwrap();
    for fixture in fixtures {
        let mut input = buy();
        for key in ["token", "budget", "minAmountOut"] {
            input[key] = fixture[key].clone();
        }
        let out = build(input);
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["data"]["inputData"], fixture["inputData"]);
    }
}
#[test]
fn offline_full_width_budget_encoding() {
    let out = build(buy());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let data = v["data"]["inputData"].as_str().unwrap();
    assert_eq!(data.len(), 2 + 8 + 6 * 64);
    assert_eq!(&data[..10], "0x995ee6da");
    assert_eq!(
        &data[74..138],
        format!("{:0>64}", "100000000000000000000000000000000")
    );
    assert_eq!(v["data"]["value"], buy()["budget"]);
}
