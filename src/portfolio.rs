//! Read-only wallet portfolio queries.
use crate::{
    client::Client,
    rwa::{self, Registry},
};
use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};
const API: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet";
const HISTORY: &str = "/bapi/defi/v2/public/wallet-direct/agent";
#[derive(Args, Clone, Debug, Default)]
pub struct BalanceArgs {
    #[arg(long)]
    pub raw: bool,
    #[arg(long)]
    pub symbol: Option<String>,
    #[arg(long = "tokenAddress")]
    pub token_address: Option<String>,
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long = "hideLowValue")]
    pub hide_low_value: Option<String>,
}
pub async fn balance(client: &mut Client, args: &BalanceArgs) -> Result<Value, String> {
    validate_balance(args)?;
    let data = client
        .request(Method::POST, &format!("{API}/mpc-wallet/token/list"), None)
        .await?;
    let rows = data
        .get("tokenList")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| json!([]));
    if args.raw {
        return Ok(rows);
    }
    let registry = rwa::fetch(client).await?;
    map_balance(&rows, &registry, args)
}
fn project(row: &Value, keys: &[&str]) -> Value {
    let mut output = serde_json::Map::new();
    for key in keys {
        if let Some(v) = row.get(*key) {
            output.insert((*key).into(), v.clone());
        }
    }
    Value::Object(output)
}
fn map_balance(rows: &Value, registry: &Registry, args: &BalanceArgs) -> Result<Value, String> {
    let hide = args
        .hide_low_value
        .as_deref()
        .map(|v| v == "true")
        .unwrap_or(args.token_address.is_none());
    let mut output = vec![];
    for row in rows.as_array().into_iter().flatten() {
        let Some(raw) = row["balance"].as_str() else {
            continue;
        };
        let Ok(mut amount) = rwa::decimal(raw) else {
            continue;
        };
        if amount <= 0 {
            continue;
        }
        let chain = row["binanceChainId"].as_str().unwrap_or("");
        let address = row["contractAddress"].as_str().unwrap_or("");
        if args.binance_chain_id.as_deref().is_some_and(|s| s != chain) {
            continue;
        }
        if args
            .symbol
            .as_deref()
            .is_some_and(|s| !row["symbol"].as_str().unwrap_or("").eq_ignore_ascii_case(s))
        {
            continue;
        }
        if args
            .token_address
            .as_deref()
            .is_some_and(|s| rwa::identity(chain, s) != rwa::identity(chain, address))
        {
            continue;
        }
        let mut price = row["price"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or("0")
            .to_owned();
        let mut balance = raw.to_owned();
        if let Some(token) = registry.token(chain, address) {
            balance = rwa::multiply_down(raw, &token.multiplier, 18)?;
            price = rwa::divide(&price, &token.multiplier, 18, false)?;
            amount = rwa::decimal(&balance)?;
        }
        let price_number = rwa::decimal(&price).unwrap_or_else(|_| 0.into());
        let value = amount.clone() * price_number;
        if amount <= 0 || (hide && value < rwa::decimal("0.01")?) {
            continue;
        }
        let mut mapped = project(row, &["symbol", "binanceChainId"]);
        mapped["address"] = Value::String(crate::client::checksum_address(address));
        mapped["balance"] = Value::String(balance);
        mapped["price"] = Value::String(price);
        mapped["value"] = Value::String(rwa::text(value));
        output.push(mapped);
    }
    Ok(Value::Array(output))
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    pub(super) async fn server(
        replies: Vec<(&'static str, Value)>,
    ) -> (Client, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = vec![];
            for (expected, data) in replies {
                let (mut socket, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut input = Vec::new();
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    input.extend_from_slice(&buf[..n]);
                    if input.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(input).unwrap();
                assert!(
                    request.starts_with(expected),
                    "expected {expected}, got {}",
                    request.lines().next().unwrap()
                );
                let body = json!({"code":"000000","data":data}).to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
                requests.push(request);
            }
            requests
        });
        let credentials = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        (Client::test_client(credentials, &origin).unwrap(), task)
    }
    #[tokio::test]
    async fn balance_raw_never_fetches_registry_or_normalizes() {
        let rows = json!([{"balance":"184467440737095516160","price":null,"contractAddress":"0xab","extra":true}]);
        let (mut client, task) = server(vec![(
            "POST /bapi/defi/v1/public/wallet-direct/agent-wallet/mpc-wallet/token/list ",
            json!({"tokenList":rows}),
        )])
        .await;
        assert_eq!(
            balance(
                &mut client,
                &BalanceArgs {
                    raw: true,
                    ..Default::default()
                }
            )
            .await
            .unwrap(),
            rows
        );
        let requests = task.await.unwrap();
        assert!(!requests[0].contains("content-length: 2"));
    }
}

#[cfg(test)]
mod normalized_tests {
    use super::*;
    #[tokio::test]
    async fn balance_rwa_types_and_exact_value() {
        let registry = json!([
         {"chainId":"56","contractAddress":"0xab","multiplier":"3","type":1},
         {"chainId":"56","contractAddress":"0xcd","multiplier":"2","type":3},
         {"chainId":"1","contractAddress":"0xab","multiplier":"4","type":777}
        ]);
        let rows = json!([
         {"symbol":"ONDO","binanceChainId":"56","contractAddress":"0xab","balance":"1","price":"1"},
         {"symbol":"BSTOCK","binanceChainId":"56","contractAddress":"0xcd","balance":"2","price":"4"},
         {"symbol":"UNKNOWN","binanceChainId":"1","contractAddress":"0xab","balance":"3","price":"8"},
         {"symbol":"TINY","binanceChainId":"56","contractAddress":"0xef","balance":"0.001","price":"1"},
         {"symbol":"ZERO","binanceChainId":"56","contractAddress":"0xef","balance":"0","price":"100"},
         {"symbol":"INVALID","binanceChainId":"56","contractAddress":"0xef","balance":"NaN","price":"100"}
        ]);
        let (mut client,task)=super::tests::server(vec![
   ("POST /bapi/defi/v1/public/wallet-direct/agent-wallet/mpc-wallet/token/list ",json!({"tokenList":rows})),
   ("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ",registry)
  ]).await;
        let out = balance(&mut client, &BalanceArgs::default()).await.unwrap();
        assert_eq!(out.as_array().unwrap().len(), 3);
        assert_eq!(out[0]["balance"], "3");
        assert_eq!(out[0]["price"], "0.333333333333333333");
        assert_eq!(out[0]["value"], "0.999999999999999999");
        assert_eq!(out[1]["balance"], "4");
        assert_eq!(out[1]["price"], "2");
        assert_eq!(out[2]["balance"], "12");
        assert_eq!(out[2]["price"], "2");
        assert!(out[0].get("rawBalance").is_none());
        task.await.unwrap();
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    #[test]
    fn filters_chain_symbol_address_and_threshold() {
        let rows = json!([
        {"symbol":"LOW","binanceChainId":"56","contractAddress":"0xABC","balance":"0.009999999999999999999","price":"1"},
        {"symbol":"LOW","binanceChainId":"1","contractAddress":"0xABC","balance":"9","price":"1"},
        {"symbol":"sol","binanceChainId":"CT_501","contractAddress":"AbCd","balance":"1"},
        {"symbol":"sol","binanceChainId":"CT_501","contractAddress":"abcd","balance":"2"},
        {"symbol":"BOUND","binanceChainId":"56","contractAddress":"0xDEF","balance":"0.01","price":"1"}
        ]);
        let r = Registry::default();
        let a = BalanceArgs {
            symbol: Some("low".into()),
            token_address: Some("0xabc".into()),
            binance_chain_id: Some("56".into()),
            ..Default::default()
        };
        let out = map_balance(&rows, &r, &a).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert_eq!(out[0]["balance"], "0.009999999999999999999");
        let out = map_balance(
            &rows,
            &r,
            &BalanceArgs {
                token_address: Some("AbCd".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert_eq!(out[0]["balance"], "1");
        let out = map_balance(&rows, &r, &BalanceArgs::default()).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn invalid_balance_args_before_requests() {
        let (mut c, t) = super::tests::server(vec![]).await;
        t.await.unwrap();
        for a in [
            BalanceArgs {
                hide_low_value: Some("yes".into()),
                ..Default::default()
            },
            BalanceArgs {
                token_address: Some("bad!".into()),
                ..Default::default()
            },
            BalanceArgs {
                binance_chain_id: Some(" ".into()),
                ..Default::default()
            },
        ] {
            let err = balance(&mut c, &a).await.unwrap_err();
            assert!(err.contains("invalid"), "{err}");
        }
    }
}

fn chain_id(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 64 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        Err("invalid binanceChainId".into())
    } else {
        Ok(())
    }
}
fn validate_balance(a: &BalanceArgs) -> Result<(), String> {
    if let Some(v) = &a.hide_low_value {
        if v != "true" && v != "false" {
            return Err("invalid hideLowValue: expected true or false".into());
        }
    }
    if let Some(v) = &a.binance_chain_id {
        chain_id(v)?
    }
    if let Some(v) = &a.symbol {
        if v.trim() != v || v.is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
            return Err("invalid symbol".into());
        }
    }
    if let Some(v) = &a.token_address {
        let evm =
            v.len() == 42 && v.starts_with("0x") && v[2..].bytes().all(|b| b.is_ascii_hexdigit());
        let sol = (32..=44).contains(&v.len())
            && v.bytes().all(|b| {
                b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b)
            });
        if !evm && !sol {
            return Err("invalid tokenAddress".into());
        }
        if let Some(chain) = &a.binance_chain_id {
            if (chain == "CT_501" && !sol) || (chain != "CT_501" && !evm) {
                return Err("invalid tokenAddress for chain".into());
            }
        }
        if evm
            && v[2..].bytes().any(|b| b.is_ascii_uppercase())
            && crate::client::checksum_address(v) != *v
        {
            return Err("invalid tokenAddress checksum".into());
        }
    }
    Ok(())
}

#[derive(Args, Clone, Debug)]
pub struct HistoryArgs {
    #[arg(long = "type", default_value = "all")]
    pub history_type: String,
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub size: u16,
    #[arg(long = "nextCursor")]
    pub next_cursor: Option<String>,
    #[arg(long)]
    pub tx: Option<String>,
    #[arg(long = "startTime")]
    pub start_time: Option<String>,
    #[arg(long = "endTime")]
    pub end_time: Option<String>,
}
impl Default for HistoryArgs {
    fn default() -> Self {
        Self {
            history_type: "all".into(),
            binance_chain_id: None,
            size: 20,
            next_cursor: None,
            tx: None,
            start_time: None,
            end_time: None,
        }
    }
}
pub async fn history(client: &mut Client, args: &HistoryArgs) -> Result<Value, String> {
    let start = validate_history(args)?;
    if let Some(hash) = &args.tx {
        let mut pairs = vec![("txHash".into(), hash.clone())];
        if let Some(chain) = &args.binance_chain_id {
            pairs.push(("binanceChainId".into(), chain.clone()))
        }
        let row = client
            .request(
                Method::GET,
                &query_path(&format!("{HISTORY}/tx-status"), &pairs),
                None,
            )
            .await?;
        if row.is_null() {
            return Ok(Value::Null);
        }
        let mut out = project(&row, &["binanceChainId", "txHash"]);
        out["status"] =
            Value::String(normalize_status(row["status"].as_str().unwrap_or("")).into());
        if let Some(time) = row.get("txTime") {
            out["txTime"] = iso_time(time)
        }
        return Ok(out);
    }
    let registry = rwa::fetch(client).await?;

    if args.history_type != "all" {
        let page = history_page(
            client,
            args,
            &args.history_type,
            args.size,
            start,
            args.next_cursor.as_deref(),
        )
        .await?;
        return Ok(history_output(&page, &registry));
    }
    if let Some(cursor) = args
        .next_cursor
        .as_deref()
        .and_then(|s| s.strip_prefix("__CONFIRMED__:"))
    {
        let mut page =
            history_page(client, args, "confirmed", args.size, start, Some(cursor)).await?;
        prefix_confirmed_cursor(&mut page);
        return Ok(history_output(&page, &registry));
    }
    let pending = history_page(
        client,
        args,
        "pending",
        args.size,
        start,
        args.next_cursor.as_deref(),
    )
    .await?;
    let rows = pending["list"].as_array().cloned().unwrap_or_default();
    if rows.len() >= usize::from(args.size) {
        let cursor = pending["nextToken"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("__CONFIRMED__:");
        return Ok(history_output(
            &json!({"list":rows.into_iter().take(usize::from(args.size)).collect::<Vec<_>>(),"nextToken":cursor}),
            &registry,
        ));
    }
    let mut confirmed = history_page(
        client,
        args,
        "confirmed",
        args.size - rows.len() as u16,
        start,
        None,
    )
    .await?;
    prefix_confirmed_cursor(&mut confirmed);
    let mut combined = rows;
    combined.extend(confirmed["list"].as_array().into_iter().flatten().cloned());
    confirmed["list"] = Value::Array(combined);
    Ok(history_output(&confirmed, &registry))
}
fn prefix_confirmed_cursor(page: &mut Value) {
    if let Some(cursor) = page["nextToken"].as_str().filter(|s| !s.is_empty()) {
        page["nextToken"] = Value::String(format!("__CONFIRMED__:{cursor}"));
    } else if let Some(map) = page.as_object_mut() {
        map.remove("nextToken");
    }
}
fn query_path(path: &str, pairs: &[(String, String)]) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().map(|(k, v)| (k, v)))
        .finish();
    format!("{path}?{query}")
}
async fn history_page(
    client: &mut Client,
    args: &HistoryArgs,
    kind: &str,
    size: u16,
    start: i64,
    cursor: Option<&str>,
) -> Result<Value, String> {
    let endpoint = if kind == "pending" {
        "tx-history-pending"
    } else {
        "tx-history-confirm"
    };
    let mut pairs = vec![
        ("hideRisk".into(), "true".into()),
        ("hideLowValue".into(), "true".into()),
        ("size".into(), size.to_string()),
        ("startTime".into(), start.to_string()),
    ];
    if let Some(end) = &args.end_time {
        pairs.push(("endTime".into(), end.clone()))
    }
    if let Some(chain) = &args.binance_chain_id {
        pairs.push(("binanceChainIds".into(), chain.clone()))
    }
    if let Some(cursor) = cursor.filter(|s| !s.is_empty()) {
        pairs.push(("nextToken".into(), cursor.into()))
    }
    let page = client
        .request(
            Method::GET,
            &query_path(&format!("{HISTORY}/{endpoint}"), &pairs),
            None,
        )
        .await?;
    if !page.is_object()
        || page
            .get("list")
            .is_some_and(|v| !v.is_array() && !v.is_null())
        || page
            .get("nextToken")
            .is_some_and(|v| !v.is_string() && !v.is_null())
    {
        return Err("invalid transaction history response".into());
    }
    Ok(page)
}
fn history_output(page: &Value, registry: &Registry) -> Value {
    let transactions = page["list"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| map_transaction(v, registry))
        .collect::<Vec<_>>();
    let mut out = json!({"transactions":transactions,"hasMore":page["nextToken"].as_str().is_some_and(|s|!s.is_empty())});
    if let Some(token) = page.get("nextToken") {
        out["nextCursor"] = token.clone()
    }
    out
}
fn iso_time(value: &Value) -> Value {
    let timestamp = value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()));
    timestamp
        .filter(|n| *n != 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| {
            Value::String(
                t.with_timezone(&chrono::Local)
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            )
        })
        .unwrap_or(Value::Null)
}
fn map_transaction(row: &Value, registry: &Registry) -> Value {
    let mut out = project(row, &["txType", "txHash", "binanceChainId", "status"]);
    if let Some(t) = row.get("txTime") {
        out["txTime"] = iso_time(t)
    }
    if let Some(hashes) = row["txHashList"].as_array().filter(|v| !v.is_empty()) {
        out["txHashList"] = Value::Array(
            hashes
                .iter()
                .map(|h| {
                    let mut mapped = project(h, &["binanceChainId", "txHash", "status"]);
                    for key in ["networkFee", "instructions"] {
                        if h.get(key).is_some_and(Value::is_null) {
                            mapped[key] = Value::Null;
                        }
                    }
                    if let Some(fee) = h.get("networkFee").filter(|v| !v.is_null()) {
                        let mut f = project(
                            fee,
                            &[
                                "binanceChainId",
                                "feeTokenAddress",
                                "feeTokenSymbol",
                                "feeValue",
                                "feeTokenDecimals",
                            ],
                        );
                        checksum_field(&mut f, "feeTokenAddress");
                        mapped["networkFee"] = f;
                    }
                    if let Some(instructions) = h.get("instructions").filter(|v| !v.is_null()) {
                        let mut items = json!({});
                        for key in ["send", "receive"] {
                            if let Some(list) = instructions.get(key) {
                                items[key] = if let Some(list) = list.as_array() {
                                    Value::Array(
                                        list.iter()
                                            .map(|i| map_instruction(i, registry, false))
                                            .collect(),
                                    )
                                } else {
                                    list.clone()
                                };
                            }
                        }
                        if let Some(approve) = instructions.get("approve") {
                            items["approve"] = if let Some(list) = approve.as_array() {
                                Value::Array(
                                    list.iter()
                                        .map(|i| map_instruction(i, registry, true))
                                        .collect(),
                                )
                            } else if approve.is_null() {
                                Value::Null
                            } else {
                                map_instruction(approve, registry, true)
                            };
                        }
                        mapped["instructions"] = items;
                    }
                    mapped
                })
                .collect(),
        );
    }
    out
}
fn checksum_field(value: &mut Value, key: &str) {
    if let Some(s) = value.get(key).and_then(Value::as_str) {
        value[key] = Value::String(crate::client::checksum_address(s));
    }
}
fn map_instruction(row: &Value, registry: &Registry, approve: bool) -> Value {
    let mut out = project(row, &["binanceChainId", "amount"]);
    let address_key = if approve { "to" } else { "addressInfo" };
    if let Some(address) = row.get(address_key) {
        if address.is_null() {
            out[address_key] = Value::Null
        } else {
            let mut a = project(address, &["address"]);
            checksum_field(&mut a, "address");
            out[address_key] = a;
        }
    }
    if let Some(token) = row.get("tokenInfo") {
        if token.is_null() {
            out["tokenInfo"] = Value::Null;
            return out;
        }
        let mut info = project(
            token,
            &[
                "binanceChainId",
                "contractAddress",
                "symbol",
                "tokenId",
                "decimals",
            ],
        );
        checksum_field(&mut info, "contractAddress");
        out["tokenInfo"] = info;
        if !approve {
            if let (Some(amount), Some(decimals), Some(address)) = (
                row["amount"].as_str(),
                token["decimals"].as_i64(),
                token["contractAddress"].as_str(),
            ) {
                if (0..=10000).contains(&decimals) && amount.len() <= 10000 {
                    if let Ok(integer) = amount.parse::<num_bigint::BigInt>() {
                        let base = rwa::text(bigdecimal::BigDecimal::new(integer, decimals));
                        let chain = token["binanceChainId"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .or_else(|| row["binanceChainId"].as_str());
                        let amount = chain
                            .and_then(|c| registry.token(c, address))
                            .and_then(|t| rwa::multiply_down(&base, &t.multiplier, 18).ok())
                            .unwrap_or(base);
                        out["amount"] = Value::String(amount);
                    }
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod history_mapping_tests {
    use super::*;
    #[test]
    fn history_projection_base_units_rwa_status_and_optional_fields() {
        let registry = Registry::from_rows(
            &json!([{"chainId":"56","contractAddress":"0xabc","multiplier":"1.25","type":1}]),
        );
        let row = json!({"txType":"send","txHash":"h","txTime":1704067200123i64,"binanceChainId":"56","status":"partial_success","secret":"drop","txHashList":[{"binanceChainId":"56","txHash":"h2","status":"submitted","networkFee":{"feeValue":"184467440737095516160","feeTokenDecimals":18,"private":"drop"},"instructions":{"send":[{"binanceChainId":"56","amount":"184467440737095516160","addressInfo":{"address":"0xabc","name":"drop"},"tokenInfo":{"binanceChainId":"56","contractAddress":"0xabc","decimals":18,"symbol":"RWA","private":"drop"}}],"receive":[{"amount":"1000000000000000000","tokenInfo":{"binanceChainId":"1","contractAddress":"0xabc","decimals":18}}],"approve":{"amount":"115792089237316195423570985008687907853269984665640564039457584007913129639935","to":{"address":"0xabc","name":"drop"},"tokenInfo":{"contractAddress":"0xabc","decimals":18}},"unknown":"drop"}}]});
        let out = map_transaction(&row, &registry);
        assert_eq!(out["status"], "partial_success");
        assert_eq!(out["txHashList"][0]["status"], "submitted");
        assert_eq!(out["txTime"], "2024-01-01T00:00:00+00:00");
        assert_eq!(
            out["txHashList"][0]["instructions"]["send"][0]["amount"],
            "230.5843009213693952"
        );
        assert_eq!(
            out["txHashList"][0]["instructions"]["receive"][0]["amount"],
            "1"
        );
        assert_eq!(
            out["txHashList"][0]["instructions"]["approve"]["amount"],
            row["txHashList"][0]["instructions"]["approve"]["amount"]
        );
        assert!(out.get("secret").is_none());
        assert!(out["txHashList"][0]["networkFee"].get("private").is_none());
        assert!(out["txHashList"][0]["instructions"]
            .get("unknown")
            .is_none());
        let partial = map_transaction(
            &json!({"status":null,"txHashList":[{"instructions":{"send":[{"amount":null,"tokenInfo":{"symbol":null}}]}}]}),
            &registry,
        );
        assert!(partial.get("txHash").is_none());
        assert!(partial.get("txTime").is_none());
        assert!(partial["status"].is_null());
        assert!(partial["txHashList"][0]["instructions"]["send"][0]
            .get("binanceChainId")
            .is_none());
        assert!(
            partial["txHashList"][0]["instructions"]["send"][0]["tokenInfo"]
                .get("decimals")
                .is_none()
        );
    }
}

#[cfg(test)]
mod history_pending_tests {
    use super::*;
    #[tokio::test]
    async fn pending_history_default_ninety_days_and_encoded_query() {
        let (mut c,t)=super::tests::server(vec![
   ("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ",json!([])),
   ("GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-pending?",json!({"list":[{"txHash":"h","status":"processing"}],"nextToken":"abc+/="}))
  ]).await;
        let before = chrono::Utc::now().timestamp_millis() - 90 * 24 * 60 * 60 * 1000;
        let out = history(
            &mut c,
            &HistoryArgs {
                history_type: "pending".into(),
                binance_chain_id: Some("56,CT_501".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let after = chrono::Utc::now().timestamp_millis() - 90 * 24 * 60 * 60 * 1000;
        assert_eq!(out["transactions"][0]["status"], "processing");
        assert_eq!(out["nextCursor"], "abc+/=");
        assert_eq!(out["hasMore"], true);
        let requests = t.await.unwrap();
        let target = requests[1]
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let u = url::Url::parse(&format!("http://localhost{target}")).unwrap();
        let q: std::collections::HashMap<_, _> = u.query_pairs().collect();
        assert_eq!(q["binanceChainIds"], "56,CT_501");
        assert_eq!(q["size"], "20");
        assert_eq!(q["hideRisk"], "true");
        assert_eq!(q["hideLowValue"], "true");
        let start = q["startTime"].parse::<i64>().unwrap();
        assert!(start >= before && start <= after);
        assert!(!q.contains_key("endTime"));
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    const REG: &str =
        "GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ";
    const PENDING: &str = "GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-pending?";
    const CONFIRMED: &str = "GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-confirm?";
    #[tokio::test]
    async fn confirmed_only_uses_raw_cursor_and_preserves_absence() {
        let (mut c, t) =
            super::tests::server(vec![(REG, json!([])), (CONFIRMED, json!({"list":[]}))]).await;
        let out = history(
            &mut c,
            &HistoryArgs {
                history_type: "confirmed".into(),
                next_cursor: Some("a+/=".into()),
                start_time: Some("1704067200000".into()),
                end_time: Some("1704153600000".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out, json!({"transactions":[],"hasMore":false}));
        let requests = t.await.unwrap();
        assert!(requests[1].contains("nextToken=a%2B%2F%3D"));
        assert!(requests[1].contains("endTime=1704153600000"));
    }
    #[tokio::test]
    async fn all_history_pending_full_then_confirmed_marker() {
        let (mut c, t) = super::tests::server(vec![
            (REG, json!([])),
            (PENDING, json!({"list":[{"txHash":"p"}]})),
        ])
        .await;
        let out = history(
            &mut c,
            &HistoryArgs {
                size: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out["nextCursor"], "__CONFIRMED__:");
        assert_eq!(out["hasMore"], true);
        t.await.unwrap();
        let (mut c, t) = super::tests::server(vec![
            (REG, json!([])),
            (
                CONFIRMED,
                json!({"list":[{"txHash":"c"}],"nextToken":"n+/="}),
            ),
        ])
        .await;
        let out = history(
            &mut c,
            &HistoryArgs {
                size: 1,
                next_cursor: Some("__CONFIRMED__:".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out["nextCursor"], "__CONFIRMED__:n+/=");
        let requests = t.await.unwrap();
        assert!(!requests[1].contains("nextToken="));
    }
    #[tokio::test]
    async fn all_underfilled_pending_fills_confirmed_and_empty_pages() {
        let (mut c, t) = super::tests::server(vec![
            (REG, json!([])),
            (
                PENDING,
                json!({"list":[{"txHash":"p"}],"nextToken":"unused"}),
            ),
            (CONFIRMED, json!({"list":[{"txHash":"c"}],"nextToken":"c2"})),
        ])
        .await;
        let out = history(
            &mut c,
            &HistoryArgs {
                size: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out["transactions"], json!([{"txHash":"p"},{"txHash":"c"}]));
        assert_eq!(out["nextCursor"], "__CONFIRMED__:c2");
        let requests = t.await.unwrap();
        assert!(requests[2].contains("size=2"));
        assert!(!requests[2].contains("nextToken="));
        let (mut c, t) = super::tests::server(vec![
            (REG, json!([])),
            (PENDING, json!({})),
            (CONFIRMED, json!({"list":[],"nextToken":null})),
        ])
        .await;
        assert_eq!(
            history(&mut c, &HistoryArgs::default()).await.unwrap(),
            json!({"transactions":[],"hasMore":false})
        );
        t.await.unwrap();
    }
    #[tokio::test]
    async fn all_full_pending_continuation_stays_pending() {
        let (mut c, t) = super::tests::server(vec![
            (REG, json!([])),
            (
                PENDING,
                json!({"list":[{"txHash":"p"},{"txHash":"extra"}],"nextToken":"p2"}),
            ),
        ])
        .await;
        let out = history(
            &mut c,
            &HistoryArgs {
                size: 1,
                next_cursor: Some("p1".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out["transactions"].as_array().unwrap().len(), 1);
        assert_eq!(out["nextCursor"], "p2");
        let requests = t.await.unwrap();
        assert!(requests[1].contains("nextToken=p1"));
    }
}

#[cfg(test)]
mod tx_status_tests {
    use super::*;
    #[tokio::test]
    async fn tx_status_only_no_registry_defaults_or_list_requests() {
        for (status, expected) in [
            ("init", "PENDING"),
            ("processing", "PENDING"),
            ("submitted", "PENDING"),
            ("confirmed", "SUCCESS"),
            ("partial_success", "SUCCESS"),
            ("submit_fail", "FAILED"),
            ("expire_fail", "FAILED"),
            ("dropped", "FAILED"),
            ("failed", "FAILED"),
            ("reverted", "FAILED"),
            ("replaced", "FAILED"),
            ("new_backend_status", "PENDING"),
        ] {
            let (mut c,t)=super::tests::server(vec![("GET /bapi/defi/v2/public/wallet-direct/agent/tx-status?txHash=synthetic-hash&binanceChainId=56 ",json!({"status":status,"binanceChainId":"56","txHash":"h","txTime":1704067200000i64,"extra":true}))]).await;
            let out = history(
                &mut c,
                &HistoryArgs {
                    tx: Some("synthetic-hash".into()),
                    binance_chain_id: Some("56".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(out["status"], expected);
            assert_eq!(out["txHash"], "h");
            assert!(out.get("extra").is_none());
            t.await.unwrap();
        }
        let (mut c, t) = super::tests::server(vec![(
            "GET /bapi/defi/v2/public/wallet-direct/agent/tx-status?txHash=h ",
            Value::Null,
        )])
        .await;
        assert_eq!(
            history(
                &mut c,
                &HistoryArgs {
                    tx: Some("h".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap(),
            Value::Null
        );
        t.await.unwrap();
    }
}

#[cfg(test)]
mod history_validation_tests {
    use super::*;
    #[tokio::test]
    async fn invalid_history_args_rejected_before_registry_or_status() {
        let (mut c, t) = super::tests::server(vec![]).await;
        t.await.unwrap();
        let cases = vec![
            HistoryArgs {
                history_type: "bad".into(),
                ..Default::default()
            },
            HistoryArgs {
                size: 0,
                ..Default::default()
            },
            HistoryArgs {
                size: 101,
                ..Default::default()
            },
            HistoryArgs {
                next_cursor: Some("<bad>".into()),
                ..Default::default()
            },
            HistoryArgs {
                next_cursor: Some("__CONFIRMED__bad".into()),
                ..Default::default()
            },
            HistoryArgs {
                next_cursor: Some("".into()),
                ..Default::default()
            },
            HistoryArgs {
                next_cursor: Some("x".repeat(1025)),
                ..Default::default()
            },
            HistoryArgs {
                start_time: Some("1700000000000oops".into()),
                ..Default::default()
            },
            HistoryArgs {
                start_time: Some("1".into()),
                ..Default::default()
            },
            HistoryArgs {
                start_time: Some("1704153600000".into()),
                end_time: Some("1704067200000".into()),
                ..Default::default()
            },
            HistoryArgs {
                binance_chain_id: Some("56,,1".into()),
                ..Default::default()
            },
            HistoryArgs {
                tx: Some("".into()),
                ..Default::default()
            },
            HistoryArgs {
                tx: Some("h".into()),
                binance_chain_id: Some("56,1".into()),
                ..Default::default()
            },
            HistoryArgs {
                tx: Some("h".into()),
                next_cursor: Some("bad space".into()),
                ..Default::default()
            },
        ];
        for a in cases {
            let err = history(&mut c, &a).await.unwrap_err();
            assert!(err.starts_with("invalid"), "{a:?}: {err}");
        }
    }
}
fn timestamp(s: &str, now: i64) -> Result<i64, String> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid timestamp in milliseconds".into());
    }
    let n = s
        .parse::<i64>()
        .map_err(|_| "invalid timestamp in milliseconds")?;
    if !(1577836800000..=now + 100 * 365 * 24 * 60 * 60 * 1000).contains(&n) {
        return Err("invalid timestamp range".into());
    }
    Ok(n)
}
fn validate_history(a: &HistoryArgs) -> Result<i64, String> {
    if !matches!(a.history_type.as_str(), "all" | "pending" | "confirmed") {
        return Err("invalid type: expected all, pending, or confirmed".into());
    }
    if !(1..=100).contains(&a.size) {
        return Err("invalid size: expected 1..100".into());
    }
    if let Some(chains) = &a.binance_chain_id {
        if a.tx.is_some() {
            chain_id(chains)?
        } else {
            for chain in chains.split(',') {
                chain_id(chain)?
            }
        }
    }
    if let Some(cursor) = &a.next_cursor {
        if cursor.is_empty()
            || cursor.len() > 1024
            || !cursor
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=_:-".contains(&b))
        {
            return Err("invalid cursor format".into());
        }
        if cursor.starts_with("__CONFIRMED__")
            && (!cursor.starts_with("__CONFIRMED__:") || a.history_type != "all")
        {
            return Err("invalid confirmed cursor for history type".into());
        }
    }
    if let Some(tx) = &a.tx {
        if tx.is_empty() || tx.len() > 512 || !tx.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("invalid transaction hash".into());
        }
    }
    let now = chrono::Utc::now().timestamp_millis();
    let start = a
        .start_time
        .as_deref()
        .map(|s| timestamp(s, now))
        .transpose()?
        .unwrap_or(now - 90 * 24 * 60 * 60 * 1000);
    if let Some(end) = &a.end_time {
        if timestamp(end, now)? < start {
            return Err("invalid time range: endTime precedes startTime".into());
        }
    }
    Ok(start)
}
/// Quota amounts are decimal strings, not lossy IEEE-754 JSON numbers.
pub async fn left_quota(client: &mut Client) -> Result<Value, String> {
    let usage = client
        .request(Method::GET, &format!("{API}/trading-limit/query"), None)
        .await?;
    let settings = client
        .request(Method::GET, &format!("{API}/settings/query"), None)
        .await?;
    let used = quota_decimal(&usage["quotaUsed"]);
    let limit = quota_decimal(&settings["dailyLimit"]);
    let mut out = project(&usage, &["date"]);
    out["quotaLeft"] = Value::String(rwa::text(&limit - &used));
    out["quotaUsed"] = Value::String(rwa::text(used));
    out["dailyLimit"] = Value::String(rwa::text(limit));
    Ok(out)
}
fn quota_decimal(v: &Value) -> bigdecimal::BigDecimal {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    rwa::decimal(&text).unwrap_or_else(|_| 0.into())
}
#[cfg(test)]
mod optional_history_tests {
    use super::*;
    #[test]
    fn preserve_null_nested_optional_fields() {
        let row = json!({"txTime":null,"status":null,"txHashList":[{"networkFee":null,"instructions":null}]});
        let out = map_transaction(&row, &Registry::default());
        assert_eq!(out, row);
    }
    #[tokio::test]
    async fn confirmed_marker_nonempty_routes_only_confirmed() {
        let (mut c,t)=super::tests::server(vec![("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ",json!([])),("GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-confirm?",json!({"list":[],"nextToken":"n2"}))]).await;
        let out = history(
            &mut c,
            &HistoryArgs {
                next_cursor: Some("__CONFIRMED__:n1+/=".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(out["nextCursor"], "__CONFIRMED__:n2");
        let requests = t.await.unwrap();
        assert!(requests[1].contains("nextToken=n1%2B%2F%3D"));
    }
    #[tokio::test]
    async fn pending_explicit_null_cursor_not_absent() {
        let (mut c,t)=super::tests::server(vec![("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ",json!([])),("GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-pending?",json!({"nextToken":null}))]).await;
        assert_eq!(
            history(
                &mut c,
                &HistoryArgs {
                    history_type: "pending".into(),
                    ..Default::default()
                }
            )
            .await
            .unwrap(),
            json!({"transactions":[],"hasMore":false,"nextCursor":null})
        );
        t.await.unwrap();
    }
}
#[cfg(test)]
mod malformed_history_tests {
    use super::*;
    #[tokio::test]
    async fn malformed_pages_fail_cleanly_without_invented_history() {
        for page in [
            Value::Null,
            json!({"list":"invalid"}),
            json!({"list":[],"nextToken":42}),
        ] {
            let (mut c,t)=super::tests::server(vec![("GET /bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list ",json!([])),("GET /bapi/defi/v2/public/wallet-direct/agent/tx-history-confirm?",page)]).await;
            assert_eq!(
                history(
                    &mut c,
                    &HistoryArgs {
                        history_type: "confirmed".into(),
                        ..Default::default()
                    }
                )
                .await
                .unwrap_err(),
                "invalid transaction history response"
            );
            t.await.unwrap();
        }
    }
}
#[cfg(test)]
mod quota_tests {
    use super::*;
    #[tokio::test]
    async fn left_quota_exact_subtraction_optional_date_and_zero_fallback() {
        for (used, limit, expected) in [
            (
                json!({"quotaUsed":"184467440737095516160.123456789","date":"2026-10-28"}),
                json!({"dailyLimit":"184467440737095516161"}),
                json!({"quotaUsed":"184467440737095516160.123456789","dailyLimit":"184467440737095516161","quotaLeft":"0.876543211","date":"2026-10-28"}),
            ),
            (
                json!({"quotaUsed":"NaN"}),
                json!({}),
                json!({"quotaUsed":"0","dailyLimit":"0","quotaLeft":"0"}),
            ),
            (
                json!({"quotaUsed":"2.5","date":null}),
                json!({"dailyLimit":"1"}),
                json!({"quotaUsed":"2.5","dailyLimit":"1","quotaLeft":"-1.5","date":null}),
            ),
        ] {
            let (mut c, t) = super::tests::server(vec![
                (
                    "GET /bapi/defi/v1/public/wallet-direct/agent-wallet/trading-limit/query ",
                    used,
                ),
                (
                    "GET /bapi/defi/v1/public/wallet-direct/agent-wallet/settings/query ",
                    limit,
                ),
            ])
            .await;
            assert_eq!(left_quota(&mut c).await.unwrap(), expected);
            t.await.unwrap();
        }
    }
}
fn normalize_status(status: &str) -> &'static str {
    match status.to_ascii_lowercase().as_str() {
        "confirmed" | "partial_success" => "SUCCESS",
        "submit_fail" | "expire_fail" | "dropped" | "failed" | "reverted" | "replaced" => "FAILED",
        _ => "PENDING",
    }
}
