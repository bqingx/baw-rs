//! Preview-bound external signing; never signs locally.
use clap::{Args, Subcommand};
use serde_json::{json, Value};
#[derive(Args, Debug, Clone)]
pub struct ContractPreview {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long = "inputData")]
    pub input_data: Option<String>,
    #[arg(long = "unsignedTx")]
    pub unsigned_tx: Option<String>,
    #[arg(long = "gasLimit")]
    pub gas_limit: Option<u64>,
}
fn nonempty(s: &str) -> Result<(), String> {
    if s.is_empty() || s.trim() != s || s.chars().any(char::is_control) {
        Err("invalid empty or padded identifier".into())
    } else {
        Ok(())
    }
}
fn is_solana(chain: &str) -> bool {
    chain == "CT_501"
}
fn address(chain: &str, s: &str) -> Result<(), String> {
    if is_solana(chain) {
        if (32..=44).contains(&s.len())
            && s.bytes()
                .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
        {
            return Ok(());
        }
    } else if let Some(hex) = s.strip_prefix("0x") {
        if hex.len() == 40
            && hex.bytes().all(|b| b.is_ascii_hexdigit())
            && (hex == hex.to_ascii_lowercase() || crate::client::checksum_address(s) == s)
        {
            return Ok(());
        }
    }
    Err("invalid chain address or EIP55 checksum".into())
}
fn same_address(chain: &str, a: &str, b: &str) -> bool {
    if is_solana(chain) {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}
fn unsigned_value(s: &str) -> bool {
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
    match hex {
        Some(h) => !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()),
        None => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
    }
}
fn contract_payload(a: &ContractPreview) -> Result<Value, String> {
    nonempty(&a.binance_chain_id)?;
    address(&a.binance_chain_id, &a.from)?;
    let mut p = json!({"binanceChainId":a.binance_chain_id,"from":a.from});
    if is_solana(&a.binance_chain_id) {
        use base64::Engine;
        if a.to.is_some() || a.value.is_some() || a.input_data.is_some() || a.gas_limit.is_some() {
            return Err("EVM fields are not supported on Solana".into());
        }
        let tx = a
            .unsigned_tx
            .as_deref()
            .ok_or("Solana requires unsignedTx")?;
        if base64::engine::general_purpose::STANDARD
            .decode(tx)
            .map_err(|_| "invalid unsignedTx base64")?
            .is_empty()
        {
            return Err("empty unsignedTx".into());
        }
        p["unsignedTx"] = json!(tx);
    } else {
        if a.unsigned_tx.is_some() {
            return Err("unsignedTx is Solana only".into());
        }
        let to = a.to.as_deref().ok_or("EVM requires to")?;
        address(&a.binance_chain_id, to)?;
        p["to"] = json!(to);
        if let Some(v) = &a.value {
            if !unsigned_value(v) {
                return Err("value must be unsigned decimal or nonempty hex wei".into());
            }
            p["value"] = json!(v);
        }
        if let Some(v) = &a.input_data {
            let h = v
                .strip_prefix("0x")
                .ok_or("inputData must be 0x-prefixed even hex")?;
            if h.len() % 2 != 0 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("inputData must be even hex".into());
            }
            p["inputData"] = json!(v);
        }
        if let Some(v) = a.gas_limit {
            if !(21000..=15000000).contains(&v) {
                return Err("gasLimit must be 21000..15000000".into());
            }
            p["gasLimit"] = json!(v);
        }
    }
    Ok(p)
}
#[derive(Subcommand, Debug)]
pub enum ContractCommand {
    Preview(ContractPreview),
    Execute {
        #[arg(long = "requestId")]
        request_id: String,
    },
    /// Preview then execute in one process, only if the pinned policy matches.
    Run(ContractRun),
    /// Preview then execute in one process with no policy gate and no human digest
    /// review. Consent is still `--confirm` + a unique `--operation-id`; the preview's
    /// risks and authority changes are returned in the output for the caller to log.
    Fire(ContractFire),
}
#[derive(Args, Debug, Clone)]
pub struct ContractRun {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long = "inputData")]
    pub input_data: Option<String>,
    #[arg(long = "gasLimit")]
    pub gas_limit: Option<u64>,
    #[arg(long)]
    pub policy: std::path::PathBuf,
}
#[derive(Args, Debug, Clone)]
pub struct ContractFire {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub value: Option<String>,
    #[arg(long = "inputData")]
    pub input_data: Option<String>,
    #[arg(long = "gasLimit")]
    pub gas_limit: Option<u64>,
}

#[derive(Args, Debug, Clone)]
pub struct MessagePreview {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: String,
    #[arg(long)]
    pub message: String,
    #[arg(long = "signType")]
    pub sign_type: String,
}
#[derive(Args, Debug, Clone)]
pub struct MessageHistory {
    #[arg(long = "binanceChainId")]
    pub binance_chain_id: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
    #[arg(long = "nextToken")]
    pub next_token: Option<String>,
    #[arg(long = "startTime")]
    pub start_time: Option<u64>,
    #[arg(long = "endTime")]
    pub end_time: Option<u64>,
    #[arg(long = "sortType")]
    pub sort_type: Option<String>,
}
#[derive(Subcommand, Debug)]
pub enum SignCommand {
    Preview(MessagePreview),
    Execute {
        #[arg(long = "requestId")]
        request_id: String,
    },
    Result {
        #[arg(long = "order-id")]
        order_id: String,
    },
    History(MessageHistory),
}
use crate::client::Client;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};
const API: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet";
const MAX_CACHE: u64 = 4 * 1024 * 1024;
const PREVIEW_TTL: u64 = 300;
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(|_| "invalid local clock".into())
}
fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn value_sha(v: &Value) -> Result<String, String> {
    Ok(sha(
        &serde_json::to_vec(v).map_err(|_| "cannot digest preview")?
    ))
}
fn timestamp(v: &Value) -> Option<u64> {
    let n = v.as_u64().or_else(|| v.as_str()?.parse().ok())?;
    Some(if n >= 1_000_000_000_000 { n / 1000 } else { n })
}
async fn check_wallet(
    client: &mut Client,
    chain: &str,
    expected: Option<&str>,
) -> Result<(String, u64), String> {
    nonempty(chain)?;
    // Revalidate against this client; the value is reused only for a few seconds inside
    // this process (BAW_RS_CHECK_TTL_MS, default 3000) and dropped on any session change.
    let settings = client.cached_settings().await?;
    let mode = &settings["devMode"];
    let expires =
        timestamp(&mode["expiresAt"]).ok_or("developer mode expiry is missing or invalid")?;
    if mode["enabled"] != true || expires <= now()? {
        return Err("developer mode is disabled or expired".into());
    }
    let wallets = client.cached_addresses().await?;
    let rows = wallets["addresses"]
        .as_array()
        .ok_or("wallet addresses missing")?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r["binanceChainId"] == chain)
        .collect();
    if matches.len() != 1 {
        return Err("wallet must have one unambiguous sender on requested chain".into());
    }
    let actual = matches[0]["address"]
        .as_str()
        .ok_or("wallet sender missing")?
        .to_owned();
    address(chain, &actual)?;
    if expected.is_some_and(|e| !same_address(chain, e, &actual)) {
        return Err("sender does not match actual wallet on requested chain".into());
    }
    Ok((actual, expires))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cached {
    version: u32,
    kind: String,
    client_binding: String,
    wallet_device: u64,
    wallet_inode: u64,
    chain: String,
    sender: String,
    request_id: String,
    expires_at: u64,
    input: Value,
    output: Value,
}
impl Cached {
    fn digest(&self) -> Result<String, String> {
        value_sha(&serde_json::to_value(self).map_err(|_| "cannot encode preview binding")?)
    }
}
struct Cache {
    dir: File,
    wallet_device: u64,
    wallet_inode: u64,
}
fn cstr(s: &std::ffi::OsStr) -> Result<CString, String> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(s.as_bytes()).map_err(|_| "invalid cache path".into())
}
fn ioerr() -> String {
    "secure preview cache I/O failed".into()
}
fn openat(parent: &File, name: &CString, flags: i32, mode: u32) -> Result<File, String> {
    // SAFETY: valid directory descriptor and NUL-terminated path; returned descriptor is owned.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        return Err(ioerr());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn private(meta: &std::fs::Metadata, dir: bool) -> Result<(), String> {
    // SAFETY: geteuid has no preconditions.
    if meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o7777 != if dir { 0o700 } else { 0o600 }
        || if dir {
            !meta.is_dir()
        } else {
            !meta.is_file() || meta.nlink() != 1
        }
    {
        return Err("preview cache must be owner-only, regular and not linked".into());
    }
    Ok(())
}
impl Cache {
    fn open(path: &Path) -> Result<Self, String> {
        if path.as_os_str().is_empty() {
            return Err("explicit wallet directory required".into());
        }
        let start = if path.is_absolute() { "/" } else { "." };
        let mut directory = File::open(start).map_err(|_| ioerr())?;
        for part in path.components() {
            match part {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(n) => {
                    directory =
                        openat(&directory, &cstr(n)?, libc::O_RDONLY | libc::O_DIRECTORY, 0)?
                }
                _ => return Err("parent path components are forbidden".into()),
            }
        }
        let meta = directory.metadata().map_err(|_| ioerr())?;
        private(&meta, true)?;
        let name = CString::new("signing-previews").unwrap();
        let made = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if made < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(ioerr());
        }
        let dir = openat(&directory, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        private(&dir.metadata().map_err(|_| ioerr())?, true)?;
        Ok(Self {
            dir,
            wallet_device: meta.dev(),
            wallet_inode: meta.ino(),
        })
    }
    fn name(id: &str) -> CString {
        CString::new(format!("{}.json", sha(id.as_bytes()))).unwrap()
    }
    fn read(&self, id: &str) -> Result<Cached, String> {
        let file = openat(
            &self.dir,
            &Self::name(id),
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        )?;
        let meta = file.metadata().map_err(|_| ioerr())?;
        private(&meta, false)?;
        if meta.len() > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_CACHE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ioerr())?;
        if bytes.len() as u64 > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "preview cache is malformed".into())
    }
    fn consumed_name(id: &str) -> CString {
        CString::new(format!("{}.consumed", sha(id.as_bytes()))).unwrap()
    }
    fn check_unconsumed(&self, id: &str) -> Result<(), String> {
        let name = Self::consumed_name(id);
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let result = unsafe {
            libc::fstatat(
                self.dir.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result == 0 {
            return Err("preview already consumed; never retry execute".into());
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
            return Err(ioerr());
        }
        Ok(())
    }
    fn consume(&self, id: &str, digest: &str) -> Result<(), String> {
        let mut marker = openat(
            &self.dir,
            &Self::consumed_name(id),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|_| "preview already consumed or consumption could not be secured")?;
        marker.write_all(digest.as_bytes()).map_err(|_| ioerr())?;
        marker.sync_all().map_err(|_| ioerr())?;
        self.dir.sync_all().map_err(|_| ioerr())
    }
    fn store(&self, cached: &Cached) -> Result<(), String> {
        let bytes = serde_json::to_vec(cached).map_err(|_| "cannot serialize preview cache")?;
        if bytes.len() as u64 > MAX_CACHE {
            return Err("preview cache too large".into());
        }
        let name = Self::name(&cached.request_id);
        let tmp = CString::new(format!(".{}.tmp", rand::random::<u128>())).unwrap();
        let mut file = openat(
            &self.dir,
            &tmp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let result: Result<(), String> = (|| {
            file.write_all(&bytes).map_err(|_| ioerr())?;
            file.sync_all().map_err(|_| ioerr())?;
            // Atomic no-clobber publication; an existing differing request ID is never replaced.
            let linked = unsafe {
                libc::linkat(
                    self.dir.as_raw_fd(),
                    tmp.as_ptr(),
                    self.dir.as_raw_fd(),
                    name.as_ptr(),
                    0,
                )
            };
            if linked < 0 {
                return Err("preview requestId cache already exists; refusing overwrite".into());
            }
            Ok(())
        })();
        unsafe {
            libc::unlinkat(self.dir.as_raw_fd(), tmp.as_ptr(), 0);
        }
        result?;
        self.dir.sync_all().map_err(|_| ioerr())
    }
}
fn failed_simulation(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().any(failed_simulation),
        Value::Object(o) => o.iter().any(|(k, v)| {
            (matches!(
                k.as_str(),
                "success" | "simulationSuccess" | "simulationPassed"
            ) && v == false)
                || (matches!(k.as_str(), "status" | "simulationStatus")
                    && v.as_str().is_some_and(|s| {
                        matches!(
                            s.to_ascii_uppercase().as_str(),
                            "FAILED" | "FAIL" | "ERROR" | "REVERTED" | "SIMULATION_FAILED"
                        )
                    }))
                || (matches!(k.as_str(), "simulationError" | "error")
                    && !v.is_null()
                    && v != false
                    && v != "")
                || (k == "code" && !v.is_null() && v != "000000" && v != "0" && v != 0)
                || (matches!(
                    k.as_str(),
                    "simulation" | "simulationResult" | "simulationDetails" | "simulationResults"
                ) && failed_simulation(v))
        }),
        _ => false,
    }
}
fn check_output(output: &Value, chain: &str, sender: &str) -> Result<(), String> {
    if !output.is_object() || output.get("previewSha256").is_some() {
        return Err("invalid preview response".into());
    }
    if failed_simulation(output) {
        return Err("preview contains failed simulation or backend failure".into());
    }
    if output.get("binanceChainId").is_some_and(|v| v != chain) {
        return Err("preview chain mismatch".into());
    }
    for key in ["from", "signer"] {
        if let Some(v) = output.get(key) {
            if !v.as_str().is_some_and(|v| same_address(chain, v, sender)) {
                return Err("preview sender mismatch".into());
            }
        }
    }
    Ok(())
}
async fn preview_call(
    client: &mut Client,
    dir: &Path,
    kind: &str,
    chain: &str,
    sender: String,
    dev_expiry: u64,
    input: Value,
) -> Result<Value, String> {
    let cache = Cache::open(dir)?;
    let output = client
        .request(
            Method::POST,
            &format!("{API}/{kind}/preview"),
            Some(input.clone()),
        )
        .await?;
    check_output(&output, chain, &sender)?;
    let request_id = output["requestId"]
        .as_str()
        .ok_or("preview requestId missing")?
        .to_owned();
    nonempty(&request_id)?;
    // The installed protocol does not require an expiry field. Always impose our
    // own five-minute limit, tightened by every server bound that is present.
    let time = now()?;
    let mut expires_at = dev_expiry.min(time.saturating_add(PREVIEW_TTL));
    for key in [
        "expiresAt",
        "expireAt",
        "expireTime",
        "requestExpireTime",
        "expiryTime",
    ] {
        if let Some(value) = output.get(key) {
            expires_at = expires_at.min(timestamp(value).ok_or("invalid preview expiry")?);
        }
    }
    if expires_at <= time {
        return Err("preview expired".into());
    }
    let record = Cached {
        version: 1,
        kind: kind.into(),
        client_binding: sha(client.credentials().client_id.as_bytes()),
        wallet_device: cache.wallet_device,
        wallet_inode: cache.wallet_inode,
        chain: chain.into(),
        sender,
        request_id,
        expires_at,
        input,
        output,
    };
    let digest = record.digest()?;
    cache.store(&record)?;
    let mut output = record.output;
    output["previewSha256"] = json!(digest);
    Ok(output)
}
async fn execute_call(
    client: &mut Client,
    dir: &Path,
    kind: &str,
    id: &str,
    accepted: Option<&str>,
) -> Result<Value, String> {
    nonempty(id)?;
    let accepted=accepted.ok_or("explicit --accept-preview-sha256 review is required, including all risks and authorityChanges")?;
    if accepted.len() != 64
        || !accepted
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid accepted preview digest".into());
    }
    let cache = Cache::open(dir)?;
    cache.check_unconsumed(id)?;
    let record = cache.read(id)?;
    if record.version != 1
        || record.kind != kind
        || record.request_id != id
        || record.client_binding != sha(client.credentials().client_id.as_bytes())
        || record.wallet_device != cache.wallet_device
        || record.wallet_inode != cache.wallet_inode
    {
        return Err("preview cache wallet, operation or request binding mismatch".into());
    }
    if record.digest()? != accepted {
        return Err("preview digest mismatch; review the exact risks and authorityChanges".into());
    }
    if record.expires_at <= now()? {
        return Err("preview expired".into());
    }
    // Consume before any network revalidation: an accepted attempt is one-shot even on error.
    cache.consume(id, accepted)?;
    check_output(&record.output, &record.chain, &record.sender)?;
    let (actual, _) = check_wallet(client, &record.chain, Some(&record.sender)).await?;
    if kind == "sign-message" {
        validate_message(&record.input, &actual)?;
    }
    if record.expires_at <= now()? {
        return Err("preview expired during wallet revalidation; preview remains consumed".into());
    }
    // The durable preview marker and operation journal prohibit retries of this POST.
    let output = client
        .request(
            Method::POST,
            &format!("{API}/{kind}/execute"),
            Some(json!({"requestId":id})),
        )
        .await?;
    if failed_simulation(&output) {
        return Err("execute backend failure; preview remains consumed, do not retry".into());
    }
    Ok(output)
}
pub async fn contract(
    client: &mut Client,
    dir: &Path,
    command: &ContractCommand,
    accepted_digest: Option<&str>,
) -> Result<Value, String> {
    match command {
        ContractCommand::Preview(a) => {
            let input = contract_payload(a)?;
            let (sender, expires) =
                check_wallet(client, &a.binance_chain_id, Some(&a.from)).await?;
            preview_call(
                client,
                dir,
                "contract-call",
                &a.binance_chain_id,
                sender,
                expires,
                input,
            )
            .await
        }
        ContractCommand::Execute { request_id } => {
            execute_call(client, dir, "contract-call", request_id, accepted_digest).await
        }
        ContractCommand::Run(a) => run_contract(client, dir, a).await,
        ContractCommand::Fire(a) => fire_contract(client, dir, a).await,
    }
}
fn chain_number(v: &Value) -> Option<u128> {
    if let Some(n) = v.as_u64() {
        return Some(n as u128);
    }
    let s = v.as_str()?;
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if h.is_empty() {
            return None;
        }
        u128::from_str_radix(h, 16).ok()
    } else if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}
fn validate_message(input: &Value, sender: &str) -> Result<(), String> {
    let chain = input["binanceChainId"]
        .as_str()
        .ok_or("message chain missing")?;
    nonempty(chain)?;
    let kind = input["signType"].as_str().ok_or("signType missing")?;
    nonempty(kind)?;
    let message = input["message"]
        .as_str()
        .ok_or("message must remain a string")?;
    if message.is_empty() {
        return Err("message is empty".into());
    }
    if kind.eq_ignore_ascii_case("EIP712") {
        if kind != "EIP712" || is_solana(chain) {
            return Err("EIP712 requires EVM and exact signType EIP712".into());
        }
        let rpc: Value = serde_json::from_str(message)
            .map_err(|_| "EIP712 requires a JSON-RPC wrapper string")?;
        if rpc["jsonrpc"] != "2.0" || rpc["method"] != "eth_signTypedData_v4" {
            return Err("EIP712 requires eth_signTypedData_v4 JSON-RPC 2.0".into());
        }
        let params = rpc["params"]
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or("typed-data wrapper requires signer and typed data")?;
        let signer = params[0].as_str().ok_or("typed-data signer missing")?;
        address(chain, signer)?;
        if !same_address(chain, signer, sender) {
            return Err("typed-data signer does not match wallet".into());
        }
        let typed = if let Some(s) = params[1].as_str() {
            serde_json::from_str::<Value>(s).map_err(|_| "invalid wrapped typed data")?
        } else {
            params[1].clone()
        };
        let domain = chain_number(&typed["domain"]["chainId"])
            .ok_or("typed-data domain chainId missing or invalid")?;
        if Some(domain) != chain_number(&json!(chain)) {
            return Err("typed-data domain chain mismatch".into());
        }
        if !typed["types"].is_object()
            || !typed["message"].is_object()
            || typed["primaryType"].as_str().is_none_or(str::is_empty)
        {
            return Err("typed-data structure missing".into());
        }
    }
    Ok(())
}
pub async fn message(
    client: &mut Client,
    dir: &Path,
    command: &SignCommand,
    accepted_digest: Option<&str>,
) -> Result<Value, String> {
    match command {
        SignCommand::Preview(a) => {
            nonempty(&a.binance_chain_id)?;
            nonempty(&a.sign_type)?;
            let input = json!({"binanceChainId":a.binance_chain_id,"message":a.message,"signType":a.sign_type});
            let (sender, expiry) = check_wallet(client, &a.binance_chain_id, None).await?;
            validate_message(&input, &sender)?;
            preview_call(
                client,
                dir,
                "sign-message",
                &a.binance_chain_id,
                sender,
                expiry,
                input,
            )
            .await
        }
        SignCommand::Execute { request_id } => {
            execute_call(client, dir, "sign-message", request_id, accepted_digest).await
        }
        SignCommand::Result { order_id } => {
            nonempty(order_id)?;
            client
                .request(
                    Method::POST,
                    &format!("{API}/sign-message/result"),
                    Some(json!({"orderId":order_id})),
                )
                .await
        }
        SignCommand::History(a) => {
            if a.limit == 0 || a.limit > 100 {
                return Err("limit must be 1..100".into());
            }
            if a.start_time.zip(a.end_time).is_some_and(|(s, e)| s > e) {
                return Err("startTime exceeds endTime".into());
            }
            if a.sort_type
                .as_deref()
                .is_some_and(|s| !matches!(s, "ASC" | "DESC"))
            {
                return Err("sortType must be ASC or DESC".into());
            }
            let query = {
                let mut q = url::form_urlencoded::Serializer::new(String::new());
                if let Some(chain) = &a.binance_chain_id {
                    nonempty(chain)?;
                    q.append_pair("binanceChainIds", chain);
                }
                q.append_pair("limit", &a.limit.to_string());
                if let Some(token) = &a.next_token {
                    nonempty(token)?;
                    q.append_pair("nextToken", token);
                }
                if let Some(t) = a.start_time {
                    q.append_pair("startTime", &t.to_string());
                }
                if let Some(t) = a.end_time {
                    q.append_pair("endTime", &t.to_string());
                }
                if let Some(sort) = &a.sort_type {
                    q.append_pair("sortType", sort);
                }
                q.finish()
            };
            client
                .request(
                    Method::GET,
                    &format!("{API}/sign-message/history?{query}"),
                    None,
                )
                .await
        }
    }
}
pub fn needs_accepted_digest(command: &ContractCommand) -> bool {
    matches!(command, ContractCommand::Execute { .. })
}
pub fn is_contract_mutating(command: &ContractCommand) -> bool {
    matches!(
        command,
        ContractCommand::Execute { .. } | ContractCommand::Run(_) | ContractCommand::Fire(_)
    )
}
pub fn is_message_mutating(command: &SignCommand) -> bool {
    matches!(command, SignCommand::Execute { .. })
}
pub fn contract_intent(command: &ContractCommand) -> Value {
    match command {
        ContractCommand::Execute { request_id } => {
            json!({"operation":"contract-call","requestId":request_id})
        }
        ContractCommand::Preview(a) => {
            json!({"operation":"contract-call-preview","binanceChainId":a.binance_chain_id,"from":a.from,"to":a.to,"value":a.value,"inputData":a.input_data,"unsignedTx":a.unsigned_tx,"gasLimit":a.gas_limit})
        }
        ContractCommand::Run(a) => {
            json!({"operation":"contract-call-run","binanceChainId":a.binance_chain_id,"from":a.from,"to":a.to,"value":a.value,"inputData":a.input_data,"gasLimit":a.gas_limit,"policy":a.policy.to_string_lossy()})
        }
        ContractCommand::Fire(a) => {
            json!({"operation":"contract-call-fire","binanceChainId":a.binance_chain_id,"from":a.from,"to":a.to,"value":a.value,"inputData":a.input_data,"gasLimit":a.gas_limit})
        }
    }
}
pub fn message_intent(command: &SignCommand) -> Value {
    match command {
        SignCommand::Execute { request_id } => {
            json!({"operation":"sign-message","requestId":request_id})
        }
        SignCommand::Preview(a) => {
            json!({"operation":"sign-message-preview","binanceChainId":a.binance_chain_id,"message":a.message,"signType":a.sign_type})
        }
        SignCommand::Result { order_id } => {
            json!({"operation":"sign-message-result","orderId":order_id})
        }
        SignCommand::History(_) => json!({"operation":"sign-message-history"}),
    }
}
/// Unattended execution: preview then execute in one process, gated by a pinned
/// policy file instead of a human digest review. Everything the policy pins is
/// checked before the preview request is sent, and the preview is checked again
/// before execution. No retries: an ambiguous execute is reported, never repeated.
async fn run_contract(
    client: &mut Client,
    dir: &Path,
    args: &ContractRun,
) -> Result<Value, String> {
    use crate::policy::{Intent, Policy};
    if is_solana(&args.binance_chain_id) {
        return Err("policy run supports EVM chains only".into());
    }
    let policy = Policy::load(&args.policy)?;
    let value = args.value.as_deref().unwrap_or("0");
    let intent = Intent {
        chain_id: &args.binance_chain_id,
        to: &args.to,
        value,
        input_data: args.input_data.as_deref(),
        gas_limit: args.gas_limit,
    };
    policy.check_intent(&intent)?;
    let preview_args = ContractPreview {
        binance_chain_id: args.binance_chain_id.clone(),
        from: args.from.clone(),
        to: Some(args.to.clone()),
        value: args.value.clone(),
        input_data: args.input_data.clone(),
        unsigned_tx: None,
        gas_limit: args.gas_limit,
    };
    let payload = contract_payload(&preview_args)?;
    let (sender, dev_expiry) =
        check_wallet(client, &args.binance_chain_id, Some(&args.from)).await?;
    let preview = preview_call(
        client,
        dir,
        "contract-call",
        &args.binance_chain_id,
        sender,
        dev_expiry,
        payload,
    )
    .await?;
    policy.check_preview(&preview)?;
    let request_id = preview["requestId"]
        .as_str()
        .ok_or("preview requestId missing")?
        .to_owned();
    let digest = preview["previewSha256"]
        .as_str()
        .ok_or("preview digest missing")?
        .to_owned();
    let execution = execute_call(client, dir, "contract-call", &request_id, Some(&digest)).await?;
    Ok(json!({
        "policy": "matched",
        "preview": {
            "requestId": request_id,
            "previewSha256": digest,
            "requireConfirmation": preview.get("requireConfirmation").cloned().unwrap_or(Value::Null),
            "gasLimit": preview.get("gasLimit").cloned().unwrap_or(Value::Null),
        },
        "execution": execution,
    }))
}

/// `contract-call fire`: preview then execute in one process, no policy file and no
/// human digest review. The caller authorises with `--confirm` + a unique
/// `--operation-id`; the preview's risk and authority data is returned so it can be
/// recorded even though nobody read it before execution.
async fn fire_contract(
    client: &mut Client,
    dir: &Path,
    args: &ContractFire,
) -> Result<Value, String> {
    if is_solana(&args.binance_chain_id) {
        return Err("fire supports EVM chains only; use preview + execute for Solana".into());
    }
    let preview_args = ContractPreview {
        binance_chain_id: args.binance_chain_id.clone(),
        from: args.from.clone(),
        to: Some(args.to.clone()),
        value: args.value.clone(),
        input_data: args.input_data.clone(),
        unsigned_tx: None,
        gas_limit: args.gas_limit,
    };
    let payload = contract_payload(&preview_args)?;
    let (sender, dev_expiry) =
        check_wallet(client, &args.binance_chain_id, Some(&args.from)).await?;
    let preview = preview_call(
        client,
        dir,
        "contract-call",
        &args.binance_chain_id,
        sender,
        dev_expiry,
        payload,
    )
    .await?;
    let request_id = preview["requestId"]
        .as_str()
        .ok_or("preview requestId missing")?
        .to_owned();
    let digest = preview["previewSha256"]
        .as_str()
        .ok_or("preview digest missing")?
        .to_owned();
    let execution = execute_call(client, dir, "contract-call", &request_id, Some(&digest)).await?;
    Ok(json!({
        "policy": "none (fire performs no policy check)",
        "preview": {
            "requestId": request_id,
            "previewSha256": digest,
            "requireConfirmation": preview.get("requireConfirmation").cloned().unwrap_or(Value::Null),
            "risks": preview.get("risks").cloned().unwrap_or(Value::Null),
            "authorityChanges": preview.get("authorityChanges").cloned().unwrap_or(Value::Null),
            "allowanceChanges": preview.get("allowanceChanges").cloned().unwrap_or(Value::Null),
        },
        "execution": execution,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    const FROM: &str = "0x1111111111111111111111111111111111111111";
    fn args() -> ContractPreview {
        ContractPreview {
            binance_chain_id: "1".into(),
            from: FROM.into(),
            to: Some("0x2222222222222222222222222222222222222222".into()),
            value: Some("0".into()),
            input_data: Some("0x".into()),
            unsigned_tx: None,
            gas_limit: None,
        }
    }
    fn fixture(
        replies: Vec<Value>,
    ) -> (
        crate::client::Client,
        Arc<Mutex<Vec<String>>>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let handle = std::thread::spawn(move || {
            for value in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= i + 4 + len {
                            break;
                        }
                    }
                }
                seen.lock().unwrap().push(String::from_utf8(bytes).unwrap());
                if let Some(ms) = value["__delay_ms"].as_u64() {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                let body = if value.get("code").is_some() {
                    value
                } else {
                    json!({"code":"000000","data":value})
                }
                .to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
            }
        });
        let credentials = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-signing-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        };
        (
            crate::client::Client::test_client(credentials, &origin).unwrap(),
            requests,
            handle,
        )
    }
    fn gates() -> Vec<Value> {
        vec![
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            json!([{"subWalletList":[{"addresses":[{"binanceChainId":"1","address":FROM}]}]}]),
            json!([]),
        ]
    }
    fn preview() -> Value {
        json!({"requestId":"../unsafe/id","expiresAt":4102444800u64,"simulationSuccess":true,"risks":[{"level":"HIGH","detail":"review"}],"authorityChanges":[{"spender":"retain"}]})
    }
    #[tokio::test]
    async fn warm_probe_never_crosses_wallet_clients() {
        struct Restore(Option<String>);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Some(v) = &self.0 {
                    std::env::set_var("BAW_RS_WARM", v);
                } else {
                    std::env::remove_var("BAW_RS_WARM");
                }
            }
        }
        let _restore = Restore(std::env::var("BAW_RS_WARM").ok());
        std::env::set_var("BAW_RS_WARM", "1");
        let (mut a, _, ta) = fixture(gates());
        let mut other = gates();
        let expected = "0x2222222222222222222222222222222222222222";
        other[1][0]["subWalletList"][0]["addresses"][0]["address"] = json!(expected);
        let (mut b, _, tb) = fixture(other);
        assert_eq!(check_wallet(&mut a, "1", None).await.unwrap().0, FROM);
        assert_eq!(check_wallet(&mut b, "1", None).await.unwrap().0, expected);
        ta.join().unwrap();
        tb.join().unwrap();
    }
    #[tokio::test]
    async fn contract_preview_preserves_risks_binds_cache_and_executes_exact_review() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            dir.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let mut replies = gates();
        replies.push(preview());
        replies.extend(gates());
        replies.push(json!({"orderId":"synthetic-order"}));
        let (mut client, seen, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(result["risks"], preview()["risks"]);
        assert_eq!(result["authorityChanges"], preview()["authorityChanges"]);
        let digest = result["previewSha256"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        let executed = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Execute {
                request_id: "../unsafe/id".into(),
            },
            Some(digest),
        )
        .await
        .unwrap();
        assert_eq!(executed, json!({"orderId":"synthetic-order"}));
        thread.join().unwrap();
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 8);
        assert!(requests[3].contains("/contract-call/preview "));
        assert!(requests[7].ends_with("{\"requestId\":\"../unsafe/id\"}"));
        use std::os::unix::fs::PermissionsExt;
        let cached = std::fs::read_dir(dir.path().join("signing-previews"))
            .unwrap()
            .map(|e| e.unwrap())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .collect::<Vec<_>>();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].file_name().to_str().unwrap().len(), 69);
        assert_eq!(
            cached[0].metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn fire_is_a_mutation_but_needs_no_accepted_digest() {
        let fire = ContractCommand::Fire(ContractFire {
            binance_chain_id: "1".into(),
            from: FROM.into(),
            to: "0x2222222222222222222222222222222222222222".into(),
            value: None,
            input_data: Some("0x".into()),
            gas_limit: None,
        });
        assert!(is_contract_mutating(&fire), "fire must require consent");
        assert!(
            !needs_accepted_digest(&fire),
            "fire has no reviewed preview to pin"
        );
    }

    #[tokio::test]
    async fn fire_previews_and_executes_in_one_call_without_a_policy() {
        let dir = tempwallet();
        let mut replies = gates();
        replies.push(preview());
        replies.extend(gates());
        replies.push(json!({"orderId":"synthetic-fire-order"}));
        let (mut client, seen, thread) = fixture(replies);
        let out = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Fire(ContractFire {
                binance_chain_id: "1".into(),
                from: FROM.into(),
                to: "0x2222222222222222222222222222222222222222".into(),
                value: Some("0".into()),
                input_data: Some("0x".into()),
                gas_limit: None,
            }),
            None,
        )
        .await
        .unwrap();
        thread.join().unwrap();
        assert_eq!(out["execution"], json!({"orderId":"synthetic-fire-order"}));
        // The risks the caller was never shown are still returned, so they can be logged.
        assert_eq!(out["preview"]["risks"], preview()["risks"]);
        assert_eq!(
            out["preview"]["authorityChanges"],
            preview()["authorityChanges"]
        );
        assert_eq!(out["preview"]["previewSha256"].as_str().unwrap().len(), 64);
        let requests = seen.lock().unwrap();
        assert_eq!(
            requests.len(),
            8,
            "strict default: gates, preview, gates again, execute"
        );
        assert!(requests[3].contains("/contract-call/preview "));
        assert!(requests[7].contains("/contract-call/execute "));
    }

    #[tokio::test]
    async fn accepted_preview_is_consumed_even_when_wallet_revalidation_fails() {
        let dir = tempwallet();
        let mut replies = gates();
        replies.push(preview());
        replies.push(json!({"devMode":{"enabled":false,"expiresAt":4102444800u64}}));
        let (mut client, seen, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        let cmd = ContractCommand::Execute {
            request_id: "../unsafe/id".into(),
        };
        assert!(contract(
            &mut client,
            dir.path(),
            &cmd,
            result["previewSha256"].as_str()
        )
        .await
        .unwrap_err()
        .contains("disabled"));
        thread.join().unwrap();
        let error = contract(
            &mut client,
            dir.path(),
            &cmd,
            result["previewSha256"].as_str(),
        )
        .await
        .unwrap_err();
        assert!(error.contains("consumed"), "{error}");
        assert_eq!(seen.lock().unwrap().len(), 5);
    }
    #[tokio::test]
    async fn preview_without_server_expiry_uses_bounded_local_ttl() {
        let dir = tempwallet();
        let mut replies = gates();
        let mut response = preview();
        response.as_object_mut().unwrap().remove("expiresAt");
        replies.push(response.clone());
        let (mut client, _, thread) = fixture(replies);
        let before = now().unwrap();
        let out = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        thread.join().unwrap();
        assert_eq!(out["risks"], response["risks"]);
        let cached = Cache::open(dir.path())
            .unwrap()
            .read("../unsafe/id")
            .unwrap();
        assert!(cached.expires_at >= before + PREVIEW_TTL);
        assert!(cached.expires_at <= now().unwrap() + PREVIEW_TTL);
        assert_eq!(out["previewSha256"], cached.digest().unwrap());
    }
    #[tokio::test]
    async fn preview_expiring_during_revalidation_never_executes() {
        let dir = tempwallet();
        let cache = Cache::open(dir.path()).unwrap();
        let record = Cached {
            version: 1,
            kind: "contract-call".into(),
            client_binding: sha(b"synthetic-signing-client"),
            wallet_device: cache.wallet_device,
            wallet_inode: cache.wallet_inode,
            chain: "1".into(),
            sender: FROM.into(),
            request_id: "expires-during-check".into(),
            expires_at: now().unwrap() + 2,
            input: contract_payload(&args()).unwrap(),
            output: preview(),
        };
        cache.store(&record).unwrap();
        let digest = record.digest().unwrap();
        let mut replies = gates();
        replies[0]["__delay_ms"] = json!(2200);
        let (mut client, seen, thread) = fixture(replies);
        let error = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Execute {
                request_id: "expires-during-check".into(),
            },
            Some(&digest),
        )
        .await
        .unwrap_err();
        thread.join().unwrap();
        assert!(error.contains("expired"), "{error}");
        assert_eq!(seen.lock().unwrap().len(), 3);
        assert!(cache.check_unconsumed("expires-during-check").is_err());
    }
    #[tokio::test]
    async fn cached_preview_digest_wallet_action_chain_and_expiry_fail_closed() {
        for case in ["digest", "client", "directory", "action", "chain", "expiry"] {
            let dir = tempwallet();
            let cache = Cache::open(dir.path()).unwrap();
            let mut output = preview();
            output["binanceChainId"] = json!("1");
            let mut record = Cached {
                version: 1,
                kind: "contract-call".into(),
                client_binding: sha(b"synthetic-signing-client"),
                wallet_device: cache.wallet_device,
                wallet_inode: cache.wallet_inode,
                chain: "1".into(),
                sender: FROM.into(),
                request_id: case.into(),
                expires_at: now().unwrap() + 300,
                input: contract_payload(&args()).unwrap(),
                output,
            };
            match case {
                "client" => record.client_binding = sha(b"another-client"),
                "directory" => record.wallet_inode += 1,
                "action" => record.kind = "sign-message".into(),
                "chain" => record.chain = "56".into(),
                "expiry" => record.expires_at = 1,
                _ => {}
            }
            cache.store(&record).unwrap();
            let digest = if case == "digest" {
                "0".repeat(64)
            } else {
                record.digest().unwrap()
            };
            let (mut client, seen, thread) = fixture(vec![]);
            thread.join().unwrap();
            let error = contract(
                &mut client,
                dir.path(),
                &ContractCommand::Execute {
                    request_id: case.into(),
                },
                Some(&digest),
            )
            .await
            .unwrap_err();
            assert!(!error.starts_with("HTTP"), "{case}: {error}");
            assert!(seen.lock().unwrap().is_empty());
        }
    }
    fn tempwallet() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            d.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        d
    }
    #[tokio::test]
    async fn failed_execute_consumes_preview_and_never_posts_twice() {
        let dir = tempwallet();
        let mut replies = gates();
        replies.push(preview());
        replies.extend(gates());
        replies.push(json!({"code":"123456","message":"synthetic-private-error"}));
        let (mut client, seen, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        let digest = result["previewSha256"].as_str().unwrap();
        let cmd = ContractCommand::Execute {
            request_id: "../unsafe/id".into(),
        };
        assert!(contract(&mut client, dir.path(), &cmd, Some(digest))
            .await
            .is_err());
        thread.join().unwrap();
        let error = contract(&mut client, dir.path(), &cmd, Some(digest))
            .await
            .unwrap_err();
        assert!(error.contains("consumed"), "{error}");
        assert_eq!(seen.lock().unwrap().len(), 8);
    }
    fn typed(chain: Value, signer: &str) -> Value {
        json!({"binanceChainId":"1","signType":"EIP712","message":json!({"jsonrpc":"2.0","method":"eth_signTypedData_v4","params":[signer,json!({"domain":{"chainId":chain},"types":{},"primaryType":"Permit","message":{}}).to_string()],"id":1}).to_string()})
    }
    #[tokio::test]
    async fn message_preview_forwards_original_wrapper_and_executes_reviewed_authority_changes() {
        let dir = tempwallet();
        let input = typed(json!(1), FROM);
        let original = format!("  {}  ", input["message"].as_str().unwrap());
        let command = SignCommand::Preview(MessagePreview {
            binance_chain_id: "1".into(),
            message: original.clone(),
            sign_type: "EIP712".into(),
        });
        assert!(!is_message_mutating(&command));
        assert_eq!(message_intent(&command)["message"], original);
        let mut replies = gates();
        replies.push(preview());
        replies.extend(gates());
        replies.push(json!({"orderId":"msg-order"}));
        let (mut client, seen, thread) = fixture(replies);
        let result = message(&mut client, dir.path(), &command, None)
            .await
            .unwrap();
        let execute = SignCommand::Execute {
            request_id: "../unsafe/id".into(),
        };
        assert!(is_message_mutating(&execute));
        assert_eq!(
            message(
                &mut client,
                dir.path(),
                &execute,
                result["previewSha256"].as_str()
            )
            .await
            .unwrap()["orderId"],
            "msg-order"
        );
        thread.join().unwrap();
        let requests = seen.lock().unwrap();
        let body: Value =
            serde_json::from_str(requests[3].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["message"], original);
        assert_eq!(result["authorityChanges"], preview()["authorityChanges"]);
    }
    #[tokio::test]
    async fn result_and_history_preserve_data_and_encode_original_query_names() {
        let dir = tempwallet();
        let (mut client, seen, thread) = fixture(vec![
            json!({"signature":"SYNTHETIC-NOT-A-SIGNATURE","risks":[1]}),
            json!({"rows":[],"nextToken":"next","authorityChanges":[1]}),
        ]);
        let result = message(
            &mut client,
            dir.path(),
            &SignCommand::Result {
                order_id: "order-test".into(),
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(result["signature"], "SYNTHETIC-NOT-A-SIGNATURE");
        let history = SignCommand::History(MessageHistory {
            binance_chain_id: Some("1,56".into()),
            limit: 7,
            next_token: Some("a&b=+".into()),
            start_time: Some(100),
            end_time: Some(200),
            sort_type: Some("DESC".into()),
        });
        assert!(!is_message_mutating(&history));
        let result = message(&mut client, dir.path(), &history, None)
            .await
            .unwrap();
        assert_eq!(result["authorityChanges"], json!([1]));
        thread.join().unwrap();
        let requests = seen.lock().unwrap();
        assert!(requests[0].ends_with("{\"orderId\":\"order-test\"}"));
        let line = requests[1].lines().next().unwrap();
        assert!(line.starts_with(&format!("GET {API}/sign-message/history?")));
        let url = url::Url::parse(&format!(
            "http://localhost{}",
            line.split_whitespace().nth(1).unwrap()
        ))
        .unwrap();
        let q = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(q["binanceChainIds"], "1,56");
        assert_eq!(q["nextToken"], "a&b=+");
        assert_eq!(q["limit"], "7");
        assert_eq!(q.len(), 6);
    }
    #[tokio::test]
    async fn risk_codes_are_preserved_not_mistaken_for_backend_failure() {
        let dir = tempwallet();
        let mut p = preview();
        p["risks"] = json!([{"code":"UNLIMITED_APPROVAL","status":"WARNING"}]);
        p["authorityChanges"] = json!([{"code":"GRANT_ROLE","spender":"synthetic"}]);
        let mut replies = gates();
        replies.push(p.clone());
        let (mut client, _, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(result["authorityChanges"], p["authorityChanges"]);
        thread.join().unwrap();
    }
    #[tokio::test]
    async fn execute_inner_backend_failure_is_error_and_request_stays_consumed() {
        let dir = tempwallet();
        let mut replies = gates();
        replies.push(preview());
        replies.extend(gates());
        replies.push(json!({"code":"EXECUTION_FAILED","message":"synthetic-private-detail"}));
        // Explicit success envelope, but failed data.code must not become fake command success.
        let last = replies.len() - 1;
        replies[last] = json!({"code":"000000","data":{"code":"EXECUTION_FAILED","message":"synthetic-private-detail"}});
        let (mut client, seen, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        let command = ContractCommand::Execute {
            request_id: "../unsafe/id".into(),
        };
        let error = contract(
            &mut client,
            dir.path(),
            &command,
            result["previewSha256"].as_str(),
        )
        .await
        .unwrap_err();
        assert!(!error.contains("private-detail"));
        thread.join().unwrap();
        assert_eq!(seen.lock().unwrap().len(), 8);
    }
    #[tokio::test]
    async fn cache_unknown_field_tampering_is_rejected_before_network() {
        let dir = tempwallet();
        let mut replies = gates();
        replies.push(preview());
        let (mut client, _, thread) = fixture(replies);
        let result = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Preview(args()),
            None,
        )
        .await
        .unwrap();
        thread.join().unwrap();
        let path = dir
            .path()
            .join("signing-previews")
            .join(Cache::name("../unsafe/id").to_str().unwrap());
        let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        v["unexpected"] = json!(true);
        std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
        let error = contract(
            &mut client,
            dir.path(),
            &ContractCommand::Execute {
                request_id: "../unsafe/id".into(),
            },
            result["previewSha256"].as_str(),
        )
        .await
        .unwrap_err();
        assert!(error.contains("malformed"), "{error}");
    }
    #[test]
    fn typed_wrapper_rejects_wrong_signer_domain_and_method() {
        assert!(validate_message(&typed(json!("0x1"), FROM), FROM).is_ok());
        assert!(validate_message(&typed(json!(56), FROM), FROM).is_err());
        assert!(validate_message(
            &typed(json!(1), "0x2222222222222222222222222222222222222222"),
            FROM
        )
        .is_err());
        let mut v = typed(json!(1), FROM);
        v["message"] = json!("{\"method\":\"personal_sign\"}");
        assert!(validate_message(&v, FROM).is_err());
    }
    #[test]
    fn evm_input_validation_rejects_bare_hex_and_bad_gas() {
        let mut args = ContractPreview {
            binance_chain_id: "1".into(),
            from: "0x1111111111111111111111111111111111111111".into(),
            to: Some("0x2222222222222222222222222222222222222222".into()),
            value: Some("0x".into()),
            input_data: Some("0x12".into()),
            unsigned_tx: None,
            gas_limit: None,
        };
        assert!(contract_payload(&args).is_err());
        args.value = Some("0x00".into());
        assert!(contract_payload(&args).is_ok());
        args.input_data = Some("0x1".into());
        assert!(contract_payload(&args).is_err());
        args.input_data = Some("0x".into());
        args.gas_limit = Some(20999);
        assert!(contract_payload(&args).is_err());
        args.gas_limit = Some(15000001);
        assert!(contract_payload(&args).is_err());
        args.gas_limit = Some(21000);
        assert_eq!(contract_payload(&args).unwrap()["gasLimit"], json!(21000));
    }
}
