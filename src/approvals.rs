//! Token approvals: list, detail, revoke. Wire protocol mirrored from `baw` 1.12.0.
//!
//! Endpoints and payloads are taken verbatim from the installed bundle
//! `/usr/lib/node_modules/@binance/agentic-wallet/dist/index.js`:
//!
//! ```text
//! var En={LIST:"/bapi/defi/v1/public/wallet-direct/agent-wallet/approve-list",
//!         DETAIL:"/bapi/defi/v1/public/wallet-direct/agent-wallet/approve-detail",
//!         RECORDS:"/bapi/defi/v1/public/wallet-direct/agent-wallet/approve-records",
//!         REVOKE:"/bapi/defi/v1/public/wallet-direct/agent-wallet/revoke"},ki=20,Pi=20,vi="MEDIUM",Qe=["approve","permit2"]
//! ```
//!
//! ```text
//! async getApprovalList(e){let n={limit:e.limit??ki};return e.binanceChainId&&(n.binanceChainId=e.binanceChainId),
//!   e.spender&&(n.spender=e.spender),e.filterTypes&&(n.filterTypes=e.filterTypes),e.offset&&(n.offset=e.offset),
//!   (await this.approvalApi.fetchApprovalList(n)).data}
//! async getApprovalDetail(e){let n=await this.getWalletAddress(),[r,o]=await Promise.all([
//!   this.approvalApi.fetchApprovalDetail({binanceChainId:e.binanceChainId,address:n,tokenContract:e.tokenContract,spender:e.spender,type:e.type}),
//!   this.approvalApi.fetchApprovalRecords({binanceChainId:e.binanceChainId,address:n,tokenContract:e.tokenContract,spender:e.spender,type:e.type,status:"all",limit:Pi})]);
//!   return{detail:r.data,records:o.data.list}}
//! async revokeApproval(e){let n=await this.getWalletAddress(),r=await this.approvalApi.fetchApprovalDetail({...});
//!   let o={binanceChainId:e.binanceChainId,tokenContract:e.tokenContract,spender:e.spender,type:e.type,feeLevel:vi};
//!   e.type==="permit2"&&r.data.permit2ContractAddress&&(o.permit2ContractAddress=r.data.permit2ContractAddress);
//!   (await this.approvalApi.submitRevoke(o)).data}
//! ```
//!
//! The wallet address comes from `(await this.walletService.getAddresses("evm"))[0]?.address`
//! (`getNetworkFromCurve(e){return e==="secp256k1"?"evm":null}`); this client reaches the
//! same list through `WalletCommand::Address` and takes the first non-Solana entry.
use crate::cli::WalletCommand;
use crate::client::{Client, API};
use reqwest::Method;
use serde_json::{json, Map, Value};

/// `ki` in the bundle: default `--limit` for `approvals list`.
const LIST_LIMIT_DEFAULT: &str = "20";
/// `Pi` in the bundle: fixed record count requested by `approvals detail`.
const RECORDS_LIMIT: &str = "20";
/// `vi` in the bundle: fixed fee level on a revoke.
const FEE_LEVEL: &str = "MEDIUM";
/// `Qe` in the bundle: the only accepted `--type` values.
const APPROVAL_TYPES: [&str; 2] = ["approve", "permit2"];
/// Risk filters documented by the installed `approvals list --help`.
const RISK_FILTERS: [&str; 4] = ["high_risk", "medium_risk", "non_interactive", "others"];

#[derive(clap::Subcommand, Debug)]
pub enum ApprovalsCommand {
    List(ApprovalsListArgs),
    Detail(ApprovalsDetailArgs),
    Revoke(ApprovalsRevokeArgs),
}

#[derive(clap::Args, Debug)]
pub struct ApprovalsListArgs {
    /// Filter by spender address
    #[arg(long, value_name = "address")]
    pub spender: Option<String>,
    /// Filter by risk type (high_risk, medium_risk, non_interactive, others)
    #[arg(long = "filterTypes", value_name = "types")]
    pub filter_types: Option<String>,
    /// Number of results to return (default: 20)
    #[arg(long, value_name = "number", default_value = LIST_LIMIT_DEFAULT)]
    pub limit: String,
    /// Pagination cursor from previous query
    #[arg(long, value_name = "cursor")]
    pub offset: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct ApprovalsDetailArgs {
    /// Binance chain ID
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: String,
    /// Token contract address
    #[arg(long = "tokenContract", value_name = "address")]
    pub token_contract: String,
    /// Spender contract address
    #[arg(long, value_name = "address")]
    pub spender: String,
    /// Approval type (approve | permit2)
    #[arg(long = "type", value_name = "type")]
    pub approval_type: String,
}

#[derive(clap::Args, Debug)]
pub struct ApprovalsRevokeArgs {
    /// Binance chain ID
    #[arg(long = "binanceChainId", value_name = "binanceChainId")]
    pub binance_chain_id: String,
    /// Token contract address
    #[arg(long = "tokenContract", value_name = "address")]
    pub token_contract: String,
    /// Spender contract address
    #[arg(long, value_name = "address")]
    pub spender: String,
    /// Approval type (approve | permit2)
    #[arg(long = "type", value_name = "type")]
    pub approval_type: String,
}

pub fn is_mutation(command: &ApprovalsCommand) -> bool {
    matches!(command, ApprovalsCommand::Revoke(_))
}

/// Intent digest input for `approvals revoke`. Deliberately network-free: it is
/// recorded in the operation journal before any request, so it carries only what
/// the user authorized. `feeLevel` is fixed client policy and
/// `permit2ContractAddress` is read from the backend during `run`, so neither
/// belongs in the intent.
pub fn mutation_payload(command: &ApprovalsCommand) -> Result<Value, String> {
    match command {
        ApprovalsCommand::Revoke(args) => {
            let target = Target::new(
                &args.binance_chain_id,
                &args.token_contract,
                &args.spender,
                &args.approval_type,
            )?;
            Ok(json!({
                "binanceChainId": target.chain,
                "tokenContract": target.token,
                "spender": target.spender,
                "type": target.kind,
            }))
        }
        _ => Err("approvals: read-only command has no mutation payload".into()),
    }
}

pub async fn run(client: &mut Client, command: &ApprovalsCommand) -> Result<Value, String> {
    match command {
        ApprovalsCommand::List(args) => list(client, args).await,
        ApprovalsCommand::Detail(args) => {
            let target = Target::new(
                &args.binance_chain_id,
                &args.token_contract,
                &args.spender,
                &args.approval_type,
            )?;
            let address = wallet_evm_address(client).await?;
            let detail = fetch_detail(client, &target, &address).await?;
            let records = fetch_records(client, &target, &address).await?;
            let mut out = Map::new();
            out.insert("detail".into(), detail);
            // Bundle: `return{detail:r.data,records:o.data.list}` — the key simply
            // disappears when the backend omits `list`, so mirror that instead of
            // inventing an empty array.
            if let Some(list) = records.get("list") {
                out.insert("records".into(), list.clone());
            }
            Ok(Value::Object(out))
        }
        ApprovalsCommand::Revoke(args) => {
            let target = Target::new(
                &args.binance_chain_id,
                &args.token_contract,
                &args.spender,
                &args.approval_type,
            )?;
            let address = wallet_evm_address(client).await?;
            let detail = fetch_detail(client, &target, &address).await?;
            // Bundle: `o={binanceChainId,tokenContract,spender,type,feeLevel:vi}` and
            // `e.type==="permit2"&&r.data.permit2ContractAddress&&(o.permit2ContractAddress=...)`.
            let mut body = json!({
                "binanceChainId": target.chain,
                "tokenContract": target.token,
                "spender": target.spender,
                "type": target.kind,
                "feeLevel": FEE_LEVEL,
            });
            if target.kind == "permit2" {
                if let Some(contract) = detail["permit2ContractAddress"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                {
                    body["permit2ContractAddress"] = json!(contract);
                }
            }
            client
                .request(Method::POST, &format!("{API}/revoke"), Some(body))
                .await
        }
    }
}

async fn list(client: &mut Client, args: &ApprovalsListArgs) -> Result<Value, String> {
    // Bundle: `let n={limit:e.limit??ki}` then spender/filterTypes/offset when set.
    // `buildQueryString` preserves that insertion order and drops empty values.
    let limit = list_limit(&args.limit)?;
    let mut params: Vec<(&str, &str)> = vec![("limit", limit)];
    if let Some(spender) = args.spender.as_deref().filter(|value| !value.is_empty()) {
        address_like(spender)?;
        params.push(("spender", spender));
    }
    if let Some(filters) = args
        .filter_types
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        risk_filters(filters)?;
        params.push(("filterTypes", filters));
    }
    if let Some(offset) = args.offset.as_deref().filter(|value| !value.is_empty()) {
        if offset.len() > 4096 || !offset.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("invalid --offset cursor".into());
        }
        params.push(("offset", offset));
    }
    // Bundle: `LIST:"/bapi/defi/v1/public/wallet-direct/agent-wallet/approve-list"`,
    // `fetchApprovalList(e){let n=this.buildQueryString(e);return this.safeRequest(`${En.LIST}${n}`,{method:"GET"})}`.
    let path = format!("{API}/approve-list{}", query_string(&params));
    let data = client.request(Method::GET, &path, None).await?;
    // Bundle JSON path prints the whole `.data` object (`g(i)`), pagination fields included.
    Ok(data)
}

async fn fetch_detail(
    client: &mut Client,
    target: &Target<'_>,
    address: &str,
) -> Result<Value, String> {
    // Bundle: `fetchApprovalDetail(e){let n=this.buildQueryString(e);return this.safeRequest(`${En.DETAIL}${n}`,{method:"GET"})}`
    // with `{binanceChainId,address,tokenContract,spender,type}`.
    let path = format!("{API}/approve-detail{}", target.query_string(address, &[]));
    client.request(Method::GET, &path, None).await
}

async fn fetch_records(
    client: &mut Client,
    target: &Target<'_>,
    address: &str,
) -> Result<Value, String> {
    // Bundle: `fetchApprovalRecords` -> `${En.RECORDS}${n}` with the detail fields
    // plus `status:"all"` and `limit:Pi` (20).
    let path = format!(
        "{API}/approve-records{}",
        target.query_string(address, &[("status", "all"), ("limit", RECORDS_LIMIT)])
    );
    client.request(Method::GET, &path, None).await
}

/// Bundle: `getWalletAddress(){let n=(await this.walletService.getAddresses("evm"))[0]?.address;
/// if(!n)throw ... "No EVM wallet address found"}`. Solana (`CT_501`) is the only
/// non-secp256k1 curve this client knows about, so the first non-Solana row is the
/// first EVM address, exactly as `getAddresses("evm")` orders it.
async fn wallet_evm_address(client: &mut Client) -> Result<String, String> {
    let wallets = client.execute(&WalletCommand::Address).await?;
    wallets["addresses"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["binanceChainId"].as_str() != Some("CT_501"))
        .and_then(|row| row["address"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| "no EVM wallet address found".into())
}

struct Target<'a> {
    chain: &'a str,
    token: &'a str,
    spender: &'a str,
    kind: &'a str,
}

impl<'a> Target<'a> {
    fn new(
        chain: &'a str,
        token: &'a str,
        spender: &'a str,
        kind: &'a str,
    ) -> Result<Self, String> {
        chain_id(chain)?;
        address_like(token)?;
        address_like(spender)?;
        if !APPROVAL_TYPES.contains(&kind) {
            return Err("invalid approval type; expected approve or permit2".into());
        }
        Ok(Self {
            chain,
            token,
            spender,
            kind,
        })
    }

    /// Bundle field order is `binanceChainId,address,tokenContract,spender,type`
    /// followed by the caller's extras (`status`,`limit` for records).
    fn query_string(&self, address: &str, extra: &[(&str, &str)]) -> String {
        let mut params: Vec<(&str, &str)> = vec![
            ("binanceChainId", self.chain),
            ("address", address),
            ("tokenContract", self.token),
            ("spender", self.spender),
            ("type", self.kind),
        ];
        params.extend_from_slice(extra);
        query_string(&params)
    }
}

/// Mirrors `buildQueryString`: `encodeURIComponent` on each key/value, `&`-joined
/// behind a leading `?`, empty when there is nothing to send.
fn query_string(params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = params
        .iter()
        .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
        .collect();
    format!("?{}", parts.join("&"))
}

/// `encodeURIComponent` leaves exactly `A-Za-z0-9-_.!~*'()` unescaped.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn list_limit(value: &str) -> Result<&str, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "invalid --limit; expected a positive integer")?;
    if parsed == 0 || value.trim() != value {
        return Err("invalid --limit; expected a positive integer".into());
    }
    Ok(value)
}

fn chain_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("invalid --binanceChainId".into());
    }
    Ok(())
}

/// Approvals span EVM and Solana spenders, so accept either shape: `0x` + 40 hex
/// digits, or a base58 address of Solana length. Anything else is rejected before
/// any request is made.
fn address_like(value: &str) -> Result<(), String> {
    let evm = value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 40 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let base58 = (32..=44).contains(&value.len())
        && value.bytes().all(|byte| {
            b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&byte)
        });
    if evm || base58 {
        Ok(())
    } else {
        Err("invalid contract or spender address".into())
    }
}

fn risk_filters(value: &str) -> Result<(), String> {
    if value.len() > 256 {
        return Err("invalid --filterTypes".into());
    }
    for filter in value.split(',') {
        if !RISK_FILTERS.contains(&filter) {
            return Err(
                "invalid --filterTypes; expected high_risk, medium_risk, non_interactive, others"
                    .into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc::{channel, Receiver},
        thread::JoinHandle,
        time::Duration,
    };

    const EVM: &str = "0x52908400098527886e0f7030069857d2e4169ee7";
    const TOKEN: &str = "0x1111111111111111111111111111111111111111";
    const SPENDER: &str = "0x2222222222222222222222222222222222222222";

    fn data(value: Value) -> Value {
        json!({ "code": "000000", "data": value })
    }

    fn serve(replies: Vec<Value>) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let replies: Vec<String> = replies.into_iter().map(|value| value.to_string()).collect();
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || {
            for body in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0, "client closed before sending a full request");
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let len = String::from_utf8_lossy(&bytes[..end])
                            .to_lowercase()
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });
        (origin, rx, handle)
    }

    fn client(origin: &str) -> Client {
        Client::test_client(
            crate::session::Credentials {
                client_id: zeroize::Zeroizing::new("synthetic-client".into()),
                session_id: zeroize::Zeroizing::new("synthetic-session".into()),
            },
            origin,
        )
        .unwrap()
    }

    fn request_line(request: &str) -> &str {
        request.lines().next().unwrap()
    }

    fn body_of(request: &str) -> Value {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    /// The two discovery calls `Client::execute(WalletCommand::Address)` makes.
    fn wallet_replies(chain: &str) -> Vec<Value> {
        vec![
            data(json!([{"subWalletList":[{"addresses":[
                {"binanceChainId":"CT_501","address":"So11111111111111111111111111111111111111112"},
                {"binanceChainId":chain,"address":EVM}
            ]}]}])),
            data(json!([
                {"binanceChainId":"56","simpleName":"BSC"},
                {"binanceChainId":"CT_501","simpleName":"SOL"}
            ])),
        ]
    }

    fn checksummed() -> String {
        crate::client::checksum_address(EVM)
    }

    #[tokio::test]
    async fn list_sends_documented_query_and_returns_backend_data() {
        let payload = json!({
            "list":[{"tokenContract":TOKEN,"spender":SPENDER,"type":"approve","riskyLevel":2,"amount":"unlimited"}],
            "total":1
        });
        let (origin, rx, server) = serve(vec![data(payload.clone())]);
        let mut client = client(&origin);
        let command = ApprovalsCommand::List(ApprovalsListArgs {
            spender: Some(SPENDER.into()),
            filter_types: Some("high_risk,others".into()),
            limit: "5".into(),
            offset: Some("cur+1=".into()),
        });

        let got = run(&mut client, &command).await.unwrap();
        assert_eq!(got, payload);

        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            format!(
                "GET {API}/approve-list?limit=5&spender={SPENDER}&filterTypes=high_risk%2Cothers&offset=cur%2B1%3D HTTP/1.1"
            )
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_defaults_to_limit_twenty() {
        let (origin, rx, server) = serve(vec![data(json!({"list":[],"total":0}))]);
        let mut client = client(&origin);
        let command = ApprovalsCommand::List(ApprovalsListArgs {
            spender: None,
            filter_types: None,
            limit: LIST_LIMIT_DEFAULT.into(),
            offset: None,
        });

        assert_eq!(
            run(&mut client, &command).await.unwrap(),
            json!({"list":[],"total":0})
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            format!("GET {API}/approve-list?limit=20 HTTP/1.1")
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn detail_reads_wallet_detail_and_recent_records() {
        let detail = json!({
            "tokenSymbol":"USDT","tokenContract":TOKEN,"spender":SPENDER,
            "type":"approve","status":"ACTIVE","amount":"unlimited",
            "permit2ContractAddress":"0x3333333333333333333333333333333333333333"
        });
        let records =
            json!([{"actionType":"revoke","amount":"12","time":1700000000,"txHash":"0xabc"}]);
        let mut replies = wallet_replies("56");
        replies.push(data(detail.clone()));
        replies.push(data(json!({"list":records.clone()})));
        let (origin, rx, server) = serve(replies);
        let mut client = client(&origin);
        let command = ApprovalsCommand::Detail(ApprovalsDetailArgs {
            binance_chain_id: "56".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "approve".into(),
        });

        let got = run(&mut client, &command).await.unwrap();
        assert_eq!(got, json!({"detail":detail,"records":records}));

        assert_eq!(
            request_line(&rx.recv().unwrap()),
            format!("POST {API}/mpc-wallet/list HTTP/1.1")
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            "GET /bapi/defi/v1/public/wallet-direct/mgmt/agent/networks/active HTTP/1.1"
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            format!(
                "GET {API}/approve-detail?binanceChainId=56&address={}&tokenContract={TOKEN}&spender={SPENDER}&type=approve HTTP/1.1",
                checksummed()
            )
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            format!(
                "GET {API}/approve-records?binanceChainId=56&address={}&tokenContract={TOKEN}&spender={SPENDER}&type=approve&status=all&limit=20 HTTP/1.1",
                checksummed()
            )
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn revoke_reads_detail_then_posts_fixed_fee_and_permit2_contract() {
        let permit2 = "0x3333333333333333333333333333333333333333";
        let mut replies = wallet_replies("56");
        replies.push(data(
            json!({"type":"permit2","permit2ContractAddress":permit2}),
        ));
        replies.push(data(json!({"txHash":"0xsynthetic"})));
        let (origin, rx, server) = serve(replies);
        let mut client = client(&origin);
        let command = ApprovalsCommand::Revoke(ApprovalsRevokeArgs {
            binance_chain_id: "56".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "permit2".into(),
        });

        assert_eq!(
            run(&mut client, &command).await.unwrap(),
            json!({"txHash":"0xsynthetic"})
        );

        let _wallets = rx.recv().unwrap();
        let _networks = rx.recv().unwrap();
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            format!(
                "GET {API}/approve-detail?binanceChainId=56&address={}&tokenContract={TOKEN}&spender={SPENDER}&type=permit2 HTTP/1.1",
                checksummed()
            )
        );
        let post = rx.recv().unwrap();
        assert_eq!(request_line(&post), format!("POST {API}/revoke HTTP/1.1"));
        assert_eq!(
            body_of(&post),
            json!({
                "binanceChainId":"56","tokenContract":TOKEN,"spender":SPENDER,
                "type":"permit2","feeLevel":"MEDIUM","permit2ContractAddress":permit2
            })
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn approve_revoke_omits_permit2_contract() {
        let mut replies = wallet_replies("1");
        replies.push(data(json!({"type":"approve"})));
        replies.push(data(json!({"txHash":"0xsynthetic"})));
        let (origin, rx, server) = serve(replies);
        let mut client = client(&origin);
        let command = ApprovalsCommand::Revoke(ApprovalsRevokeArgs {
            binance_chain_id: "1".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "approve".into(),
        });

        run(&mut client, &command).await.unwrap();
        let _wallets = rx.recv().unwrap();
        let _networks = rx.recv().unwrap();
        let _detail = rx.recv().unwrap();
        let body = body_of(&rx.recv().unwrap());
        assert_eq!(
            body,
            json!({
                "binanceChainId":"1","tokenContract":TOKEN,"spender":SPENDER,
                "type":"approve","feeLevel":"MEDIUM"
            })
        );
        assert!(body.get("permit2ContractAddress").is_none());
        server.join().unwrap();
    }

    #[test]
    fn only_revoke_is_a_mutation_and_it_carries_its_intent() {
        let list = ApprovalsCommand::List(ApprovalsListArgs {
            spender: None,
            filter_types: None,
            limit: LIST_LIMIT_DEFAULT.into(),
            offset: None,
        });
        let detail = ApprovalsCommand::Detail(ApprovalsDetailArgs {
            binance_chain_id: "56".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "approve".into(),
        });
        let revoke = ApprovalsCommand::Revoke(ApprovalsRevokeArgs {
            binance_chain_id: "56".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "approve".into(),
        });

        assert!(!is_mutation(&list));
        assert!(!is_mutation(&detail));
        assert!(is_mutation(&revoke));
        assert!(mutation_payload(&list).is_err());
        assert!(mutation_payload(&detail).is_err());
        assert_eq!(
            mutation_payload(&revoke).unwrap(),
            json!({
                "binanceChainId":"56","tokenContract":TOKEN,"spender":SPENDER,"type":"approve"
            })
        );
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let (origin, rx, server) = serve(vec![json!({
            "code":"100999","message":"do-not-echo-backend-text","data":null
        })]);
        let mut client = client(&origin);
        let command = ApprovalsCommand::List(ApprovalsListArgs {
            spender: None,
            filter_types: None,
            limit: LIST_LIMIT_DEFAULT.into(),
            offset: None,
        });

        let error = run(&mut client, &command).await.unwrap_err();
        assert_eq!(error, "Binance API code 100999");
        assert!(!error.contains("do-not-echo-backend-text"));
        rx.recv().unwrap();
        server.join().unwrap();
    }

    #[tokio::test]
    async fn invalid_arguments_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = client(&origin);

        let zero_limit = ApprovalsCommand::List(ApprovalsListArgs {
            spender: None,
            filter_types: None,
            limit: "0".into(),
            offset: None,
        });
        assert!(run(&mut client, &zero_limit)
            .await
            .unwrap_err()
            .contains("--limit"));

        let bad_type = ApprovalsCommand::Detail(ApprovalsDetailArgs {
            binance_chain_id: "56".into(),
            token_contract: TOKEN.into(),
            spender: SPENDER.into(),
            approval_type: "transfer".into(),
        });
        assert!(run(&mut client, &bad_type)
            .await
            .unwrap_err()
            .contains("approval type"));

        let bad_address = ApprovalsCommand::Revoke(ApprovalsRevokeArgs {
            binance_chain_id: "56".into(),
            token_contract: "not-an-address".into(),
            spender: SPENDER.into(),
            approval_type: "approve".into(),
        });
        assert!(run(&mut client, &bad_address)
            .await
            .unwrap_err()
            .contains("address"));

        assert!(listener.accept().is_err());
    }
}
