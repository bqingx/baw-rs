//! Explicit, read-only WSS diagnostics. Never loads a wallet or signs a transaction.
//! `backfill` deliberately probes a 21-block range; `shared-backfill` pages it safely.
use baw_rs::chain_rpc::SharedRpc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{os::unix::fs::MetadataExt, path::Path, sync::Arc, time::Duration};
use tokio_tungstenite::tungstenite::{Error, Message};

fn transport_error(error: &Error) -> String {
    match error {
        Error::Http(response) => format!("HTTP {}", response.status().as_u16()),
        Error::Io(error) => format!("IO {:?}", error.kind()),
        Error::Tls(_) => "TLS error".into(),
        Error::Protocol(_) => "WebSocket protocol error".into(),
        _ => "WebSocket transport error".into(),
    }
}

fn endpoint(path: &Path) -> Result<String, String> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| "cannot inspect profile")?;
    if !path.is_absolute()
        || path.canonicalize().map_err(|_| "cannot resolve profile")? != path
        || !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() > 65536
    {
        return Err("profile must be private, owner-owned, absolute and unaliased".into());
    }
    let profile: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "cannot read profile")?)
            .map_err(|_| "invalid profile JSON")?;
    let url = profile["rpc_url"].as_str().ok_or("profile needs rpc_url")?;
    let url = url.replacen("https://", "wss://", 1);
    Arc::new(SharedRpc::default()).acquire(&url)?;
    Ok(url)
}

fn log_filter() -> Value {
    // Public BSC USDT mint logs; no user-specific account or credentials in this example.
    json!({"address":"0x55d398326f99059ff775485246999027b3197955","topics":[
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        format!("0x{}","00".repeat(32))
    ]})
}
fn head(value: &Value) -> Result<u64, String> {
    let text = value
        .as_str()
        .and_then(|s| s.strip_prefix("0x"))
        .ok_or("invalid head")?;
    u64::from_str_radix(text, 16).map_err(|_| "invalid head".into())
}

async fn probe(mode: &str, url: &str) -> Result<Value, String> {
    if mode.starts_with("shared") {
        let manager = Arc::new(SharedRpc::default());
        let lease = manager.acquire(url)?;
        let chain = lease.read("eth_chainId", json!([])).await?;
        if mode == "shared" {
            return Ok(json!({"mode":mode,"chainId":chain}));
        }
        let subscription = lease.subscribe(json!(["logs", log_filter()])).await?;
        let tip = head(&lease.read("eth_blockNumber", json!([])).await?)?;
        let mut cursor = tip.saturating_sub(20);
        let mut pages = Vec::new();
        let mut count = 0;
        loop {
            let end = tip.min(cursor.saturating_add(9));
            let mut filter = log_filter();
            filter["fromBlock"] = json!(format!("0x{cursor:x}"));
            filter["toBlock"] = json!(format!("0x{end:x}"));
            let result = lease.read("eth_getLogs", json!([filter])).await?;
            count += result.as_array().ok_or("invalid log response")?.len();
            pages.push(json!([cursor, end]));
            if end == tip {
                break;
            }
            cursor = end.checked_add(1).ok_or("block overflow")?;
        }
        drop(subscription);
        drop(lease);
        tokio::time::sleep(Duration::from_millis(300)).await;
        return Ok(json!({"mode":mode,"chainId":chain,"logsSubscribed":true,
            "backfillPages":pages,"logCount":count,"transports":1}));
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| transport_error(&e))?;
    let method = if mode == "backfill" {
        "eth_blockNumber"
    } else {
        "eth_chainId"
    };
    socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":[]})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|e| transport_error(&e))?;
    while let Some(frame) = socket.next().await {
        match frame.map_err(|e| transport_error(&e))? {
            Message::Text(text) => {
                let body: Value = serde_json::from_str(&text).map_err(|_| "invalid JSON reply")?;
                if body["jsonrpc"] != "2.0" {
                    return Err("invalid JSON-RPC version".into());
                }
                if mode == "backfill" && body["id"] == 1 && body.get("error").is_none() {
                    let tip = head(&body["result"])?;
                    let mut filter = log_filter();
                    filter["fromBlock"] = json!(format!("0x{:x}", tip.saturating_sub(20)));
                    filter["toBlock"] = json!(format!("0x{tip:x}"));
                    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":2,"method":"eth_getLogs","params":[filter]}).to_string().into()))
                        .await.map_err(|e|transport_error(&e))?;
                    continue;
                }
                // Provider text is untrusted and may echo the secret endpoint. Emit only
                // numeric codes and a recognized static classification, never the raw body.
                let rejected = body.get("error").is_some_and(|v| !v.is_null());
                let free_limit = body
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.to_ascii_lowercase().contains("10 block range"));
                let output = json!({"mode":mode,"chainId":if mode=="wire" {body["result"].clone()} else {Value::Null},
                    "logCount":body["result"].as_array().map(Vec::len),"providerRejected":rejected,
                    "rpcErrorCode":body.pointer("/error/code").and_then(Value::as_i64),"freeTier10BlockLimit":free_limit});
                let _ = socket.close(None).await;
                return Ok(output);
            }
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .await
                .map_err(|e| transport_error(&e))?,
            Message::Close(_) => return Err("connection closed before RPC reply".into()),
            _ => {}
        }
    }
    Err("connection ended before RPC reply".into())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2
        || !matches!(
            args[0].as_str(),
            "wire" | "shared" | "backfill" | "shared-backfill"
        )
    {
        eprintln!(
            "usage: rpc_probe wire|shared|backfill|shared-backfill /absolute/private/profile.json"
        );
        std::process::exit(2);
    }
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let url = endpoint(Path::new(&args[1]))?;
        probe(&args[0], &url).await
    })
    .await;
    match result {
        Ok(Ok(output)) => {
            let success = output["providerRejected"] != true;
            println!("{}", json!({"success":success,"data":output}));
            if !success {
                std::process::exit(1);
            }
        }
        failure => {
            let error = match failure {
                Ok(Err(error)) => error,
                _ => "diagnostic timeout".into(),
            };
            println!("{}", json!({"success":false,"error":error}));
            std::process::exit(1);
        }
    }
}
