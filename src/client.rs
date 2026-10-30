//! Fixed-origin native HTTP client. Credentials never leave this object or get persisted.
use crate::cli::WalletCommand;
use crate::session::Credentials;
use crate::transport::unpack_response;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderValue, SET_COOKIE};
use reqwest::{Method, Url};
use serde_json::{json, Value};
use std::time::Duration;

const ORIGIN: &str = "https://www.binance.com";
pub(crate) const API: &str = "/bapi/defi/v1/public/wallet-direct/agent-wallet";
/// Bundle: `wss://${n.includes("qa") ? n.replace("www","nbstream") : n.replace("www","web3-stream")}/w3w/stream`
/// where `n` is the API host. Derived from the client's own origin so tests can point at
/// a loopback server; the production origin always yields the bundle's URL.
const STREAM_PATH: &str = "/w3w/stream";
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

pub struct Client {
    http: reqwest::Client,
    origin: String,
    credentials: Credentials,
    invalidated: bool,
    pub(crate) payment_consumed:
        std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// Short-lived caches for the two reads every guarded write performs before it can
    /// preview: `settings/query` (developer mode) and the wallet's chain addresses.
    /// Bounded by [`CHECK_TTL`] so a revoked developer mode cannot stay trusted for long,
    /// and cleared outright whenever the session changes.
    settings_cache: Option<(std::time::Instant, Value)>,
    addresses_cache: Option<(std::time::Instant, Value)>,
}

/// Freshness window for the pre-preview reads.
///
/// Defaults to zero: a one-shot command performs each check exactly once anyway, so
/// caching buys nothing and re-reading keeps the strict behaviour. A resident server sets
/// `BAW_RS_CHECK_TTL_MS` when it starts, which is where the saving is real — the preview
/// and the execute that follows it then share one settings read and one address read
/// instead of paying four round trips.
fn check_ttl() -> std::time::Duration {
    let ms = std::env::var("BAW_RS_CHECK_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    std::time::Duration::from_millis(ms)
}

impl Client {
    pub fn new(credentials: Credentials) -> Result<Self, String> {
        Self::build(credentials, ORIGIN)
    }

    fn build(credentials: Credentials, origin: &str) -> Result<Self, String> {
        if credentials.session_id.len() > 16_384
            || !credentials
                .session_id
                .bytes()
                .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b','))
        {
            return Err("invalid session credential".into());
        }
        let mut headers = host_headers()?;
        let mut id =
            HeaderValue::from_str(&credentials.client_id).map_err(|_| "invalid client identity")?;
        id.set_sensitive(true);
        headers.insert("agentclientid", id.clone());
        headers.insert("bnc-uuid", id);
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .default_headers(headers)
            .user_agent(concat!("baw-rs/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "failed to create HTTP client")?;
        Ok(Self {
            http,
            origin: origin.into(),
            credentials,
            invalidated: false,
            payment_consumed: Default::default(),
            settings_cache: None,
            addresses_cache: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn test_client(credentials: Credentials, origin: &str) -> Result<Self, String> {
        let url = Url::parse(origin).map_err(|_| "invalid test origin")?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("test origin must be loopback".into());
        }
        Self::build(credentials, origin)
    }

    /// `settings/query`, reused for a few seconds inside one process.
    ///
    /// Every guarded write re-reads this to confirm developer mode is enabled and
    /// unexpired. That is two round trips per command when nothing else changes, so the
    /// value is reused for [`check_ttl`]. Set `BAW_RS_CHECK_TTL_MS=0` for the strict
    /// original behaviour.
    pub(crate) async fn cached_settings(&mut self) -> Result<Value, String> {
        let ttl = check_ttl();
        self.settings_with_ttl(ttl).await
    }

    async fn settings_with_ttl(&mut self, ttl: std::time::Duration) -> Result<Value, String> {
        if !ttl.is_zero() {
            if let Some((at, value)) = &self.settings_cache {
                if at.elapsed() < ttl {
                    return Ok(value.clone());
                }
            }
        }
        let value = self
            .request(Method::GET, &format!("{API}/settings/query"), None)
            .await?;
        self.settings_cache = Some((std::time::Instant::now(), value.clone()));
        Ok(value)
    }

    /// `wallet address` (two requests: wallet list + active networks), cached the same way.
    pub(crate) async fn cached_addresses(&mut self) -> Result<Value, String> {
        let ttl = check_ttl();
        self.addresses_with_ttl(ttl).await
    }

    async fn addresses_with_ttl(&mut self, ttl: std::time::Duration) -> Result<Value, String> {
        if !ttl.is_zero() {
            if let Some((at, value)) = &self.addresses_cache {
                if at.elapsed() < ttl {
                    return Ok(value.clone());
                }
            }
        }
        let value = self.execute(&WalletCommand::Address).await?;
        self.addresses_cache = Some((std::time::Instant::now(), value.clone()));
        Ok(value)
    }

    /// Drops every cached read; call whenever the session or credentials change.
    pub(crate) fn clear_check_caches(&mut self) {
        self.settings_cache = None;
        self.addresses_cache = None;
    }

    pub(crate) fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// WebSocket endpoint for WSP pushes, derived from this client's origin.
    ///
    /// Mirrors the installed bundle: `https` → `wss`, `http` → `ws`, and the API host
    /// `www.binance.com` becomes `web3-stream.binance.com` (`nbstream` on qa hosts).
    /// Subscription authorisation comes from the pre-subscribe `subKeys`, not from the
    /// session cookie, so no credentials are placed in the socket URL.
    pub(crate) fn stream_url(&self) -> Result<String, String> {
        let url = Url::parse(&self.origin).map_err(|_| "invalid fixed API endpoint")?;
        let host = url.host_str().ok_or("invalid fixed API endpoint")?;
        let derived = if host.contains("qa") {
            host.replace("www", "nbstream")
        } else {
            host.replace("www", "web3-stream")
        };
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        Ok(format!("{scheme}://{derived}{STREAM_PATH}"))
    }

    /// Only explicit signin may replace an invalidated session with a fresh auth flow.
    pub(crate) fn begin_signin(&mut self) {
        self.credentials.session_id = zeroize::Zeroizing::new(String::new());
        self.invalidated = false;
        self.clear_check_caches();
    }

    pub(crate) fn set_session(
        &mut self,
        session: zeroize::Zeroizing<String>,
    ) -> Result<(), String> {
        if session.is_empty()
            || session.len() > 16_384
            || !session
                .bytes()
                .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b','))
        {
            return Err("invalid session credential".into());
        }
        self.credentials.session_id = session;
        self.invalidated = false;
        self.clear_check_caches();
        Ok(())
    }

    pub async fn execute(&mut self, command: &WalletCommand) -> Result<Value, String> {
        if self.invalidated {
            return Err("session invalidated; restart with a valid session".into());
        }
        if self.credentials.session_id.is_empty() {
            return if matches!(command, WalletCommand::Status) {
                Ok(json!({"status":"UNCONNECTED"}))
            } else {
                Err("wallet session is missing".into())
            };
        }
        match command {
            WalletCommand::SpeedUp(_)
            | WalletCommand::Cancel(_)
            | WalletCommand::ResolveDomain(_) => {
                Err("wallet operation is routed by dispatch, not by client".into())
            }
            WalletCommand::Status => {
                let data = self
                    .request(Method::GET, &format!("{API}/login/query"), None)
                    .await?;
                let status = if data["connectionStatus"] != "CONNECTED" {
                    "UNCONNECTED"
                } else if data["walletCreateStatus"] == "CREATED" {
                    "CONNECTED"
                } else {
                    "CREATING"
                };
                Ok(json!({"status":status}))
            }
            WalletCommand::Address => {
                let wallets = self
                    .request(Method::POST, &format!("{API}/mpc-wallet/list"), None)
                    .await?;
                let networks = self
                    .request(
                        Method::GET,
                        "/bapi/defi/v1/public/wallet-direct/mgmt/agent/networks/active",
                        None,
                    )
                    .await?;
                let mut addresses = Vec::new();
                if let Some(subwallets) = wallets
                    .get(0)
                    .and_then(|w| w.get("subWalletList"))
                    .and_then(Value::as_array)
                {
                    for subwallet in subwallets {
                        if let Some(rows) = subwallet["addresses"].as_array() {
                            for row in rows {
                                let chain = row["binanceChainId"]
                                    .as_str()
                                    .ok_or("invalid wallet address response")?;
                                let address = row["address"]
                                    .as_str()
                                    .ok_or("invalid wallet address response")?;
                                let name = networks
                                    .as_array()
                                    .and_then(|rows| {
                                        rows.iter()
                                            .find(|network| network["binanceChainId"] == chain)
                                    })
                                    .and_then(|network| network["simpleName"].as_str())
                                    .filter(|name| !name.is_empty())
                                    .unwrap_or(chain);
                                addresses.push(json!({"binanceChainId":chain,"chainName":name,"address":checksum_address(address)}));
                            }
                        }
                    }
                }
                if addresses.is_empty() {
                    return Err("wallet not created".into());
                }
                Ok(json!({"addresses":addresses}))
            }
            WalletCommand::Chains => {
                let data = self
                    .request(
                        Method::GET,
                        "/bapi/defi/v1/public/wallet-direct/mgmt/agent/networks/active",
                        None,
                    )
                    .await?;
                Ok(Value::Array(
                    data.as_array()
                        .map(|rows| {
                            rows.iter()
                                .map(|row| {
                                    let mut projected = serde_json::Map::new();
                                    for key in ["binanceChainId", "name", "simpleName"] {
                                        if let Some(value) = row.get(key) {
                                            projected.insert(key.into(), value.clone());
                                        }
                                    }
                                    Value::Object(projected)
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                ))
            }
            WalletCommand::Balance(args) => crate::portfolio::balance(self, args).await,
            WalletCommand::TxHistory(args) => crate::portfolio::history(self, args).await,
            WalletCommand::LeftQuota => crate::portfolio::left_quota(self).await,
            WalletCommand::Send(_) => Err("mutation requires guarded command dispatch".into()),
            WalletCommand::GasPrice(chain) | WalletCommand::TxLock(chain) => {
                if chain.binance_chain_id.trim().is_empty() {
                    return Err("binanceChainId must be nonempty".into());
                }
                let endpoint = if matches!(command, WalletCommand::GasPrice(_)) {
                    "/blockchain/gasPrice"
                } else {
                    "/order/tx-lock-status"
                };
                self.request(
                    Method::POST,
                    &format!("{API}{endpoint}"),
                    Some(json!({"binanceChainId":chain.binance_chain_id})),
                )
                .await
            }
            WalletCommand::AgentId => {
                self.request(Method::GET, &format!("{API}/agent-id/claimed"), None)
                    .await
            }
            WalletCommand::Settings => {
                let settings = self
                    .request(Method::GET, &format!("{API}/settings/query"), None)
                    .await?;
                let trading_limit = self
                    .request(Method::GET, &format!("{API}/trading-limit/query"), None)
                    .await?;
                let login = self
                    .request(Method::GET, &format!("{API}/login/query"), None)
                    .await?;
                Ok(json!({"settings":settings,"tradingLimit":trading_limit,"login":login}))
            }
        }
    }

    pub(crate) async fn request(
        &mut self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        if self.invalidated {
            return Err("session invalidated; restart with a valid session".into());
        }
        let url = Url::parse(&format!("{}{path}", self.origin))
            .map_err(|_| "invalid fixed API endpoint")?;
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        let trace: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut cookie = HeaderValue::from_str(&format!(
            "agentSessionId={}",
            self.credentials.session_id.as_str()
        ))
        .map_err(|_| "invalid session credential")?;
        cookie.set_sensitive(true);
        let mut request = self.http.request(method, url).header("x-trace-id", trace);
        if !self.credentials.session_id.is_empty() {
            request = request.header("cookie", cookie);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                "HTTP request timed out"
            } else {
                "HTTP request failed"
            }
        })?;
        let status = response.status().as_u16();
        if matches!(status, 401 | 403) {
            self.invalidate();
            return Err(format!("HTTP {status}; session invalidated"));
        }
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }
        let pending_cookie = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|cookie| {
                cookie
                    .split(';')
                    .next()
                    .and_then(|v| v.strip_prefix("agentSessionId="))
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            });
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err("HTTP response exceeds 4 MiB".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "failed to read HTTP response")?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err("HTTP response exceeds 4 MiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if serde_json::from_slice::<Value>(&bytes)
            .ok()
            .is_some_and(|v| {
                matches!(
                    v["code"].as_str(),
                    Some("100001005" | "10003001" | "10003002")
                )
            })
        {
            self.invalidate();
            return Err("session expired; restart with a valid session".into());
        }
        let data = unpack_response(status, &bytes)?;
        if let Some(cookie) = pending_cookie {
            if cookie.len() <= 16_384
                && cookie
                    .bytes()
                    .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b','))
            {
                self.credentials.session_id = zeroize::Zeroizing::new(cookie);
            }
        }
        Ok(data)
    }

    pub(crate) fn invalidate(&mut self) {
        self.invalidated = true;
        use zeroize::Zeroize;
        self.credentials.session_id.zeroize();
        self.clear_check_caches();
    }
}

// Keccak-f[1600], restricted to the 40 ASCII hex bytes needed by EIP-55.
// This is display checksum normalization only, never signing or encryption.
pub(crate) fn checksum_address(address: &str) -> String {
    let Some(hex) = address
        .strip_prefix("0x")
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return address.into();
    };
    let lower = hex.to_ascii_lowercase();
    let mut block = [0u8; 136];
    block[..40].copy_from_slice(lower.as_bytes());
    block[40] = 1;
    block[135] = 0x80;
    let mut a = [0u64; 25];
    for (i, chunk) in block.as_chunks::<8>().0.iter().enumerate() {
        a[i] = u64::from_le_bytes(*chunk);
    }
    const ROT: [u32; 25] = [
        0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56,
        14,
    ];
    const RC: [u64; 24] = [
        0x0000000000000001,
        0x0000000000008082,
        0x800000000000808a,
        0x8000000080008000,
        0x000000000000808b,
        0x0000000080000001,
        0x8000000080008081,
        0x8000000000008009,
        0x000000000000008a,
        0x0000000000000088,
        0x0000000080008009,
        0x000000008000000a,
        0x000000008000808b,
        0x800000000000008b,
        0x8000000000008089,
        0x8000000000008003,
        0x8000000000008002,
        0x8000000000000080,
        0x000000000000800a,
        0x800000008000000a,
        0x8000000080008081,
        0x8000000000008080,
        0x0000000080000001,
        0x8000000080008008,
    ];
    for rc in RC {
        let mut c = [0u64; 5];
        for x in 0..5 {
            for y in 0..5 {
                c[x] ^= a[x + 5 * y];
            }
        }
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                a[x + 5 * y] ^= d;
            }
        }
        let mut b = [0u64; 25];
        for x in 0..5 {
            for y in 0..5 {
                b[y + 5 * ((2 * x + 3 * y) % 5)] = a[x + 5 * y].rotate_left(ROT[x + 5 * y]);
            }
        }
        for x in 0..5 {
            for y in 0..5 {
                a[x + 5 * y] = b[x + 5 * y] ^ ((!b[(x + 1) % 5 + 5 * y]) & b[(x + 2) % 5 + 5 * y]);
            }
        }
        a[0] ^= rc;
    }
    let hash: Vec<u8> = a[..4].iter().flat_map(|lane| lane.to_le_bytes()).collect();
    let mut result = String::from("0x");
    for (i, byte) in lower.bytes().enumerate() {
        let nibble = if i % 2 == 0 {
            hash[i / 2] >> 4
        } else {
            hash[i / 2] & 0xf
        };
        result.push(if nibble >= 8 {
            byte.to_ascii_uppercase()
        } else {
            byte
        } as char);
    }
    result
}

fn external_ipv4_mac() -> Option<String> {
    let mut head = std::ptr::null_mut();
    // SAFETY: getifaddrs initializes an owned linked list, freed after traversal.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return None;
    }
    let mut cursor = head;
    let mut found = None;
    while !cursor.is_null() {
        // SAFETY: cursor and its address/name pointers belong to the live list.
        let interface = unsafe { &*cursor };
        if !interface.ifa_addr.is_null()
            && interface.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
            && unsafe { (*interface.ifa_addr).sa_family as i32 } == libc::AF_INET
        {
            let name = unsafe { std::ffi::CStr::from_ptr(interface.ifa_name) }.to_string_lossy();
            if let Ok(mac) = std::fs::read_to_string(format!("/sys/class/net/{name}/address")) {
                let mac = mac.trim();
                if mac.len() == 17 && mac != "00:00:00:00:00:00" {
                    found = Some(mac.to_owned());
                    break;
                }
            }
        }
        cursor = interface.ifa_next;
    }
    // SAFETY: head is the original list from successful getifaddrs.
    unsafe {
        libc::freeifaddrs(head);
    }
    found
}

fn host_headers() -> Result<HeaderMap, String> {
    let read = |path: &str| {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let release = read("/proc/sys/kernel/osrelease");
    let hostname = read("/proc/sys/kernel/hostname");
    let os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    };
    let mac = external_ipv4_mac()
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "unknown".into()));
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "en-US".into())
        .split('.')
        .next()
        .unwrap_or("en-US")
        .replace('_', "-");
    let timezone = std::env::var("TZ")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let value = read("/etc/timezone");
            if value.is_empty() {
                "UTC".into()
            } else {
                value
            }
        });
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("content-type", "application/json".into()),
        ("agentclientsystem", format!("{os} {release}")),
        ("agentclientmac", mac),
        ("device_name", hostname),
        ("system_lang", locale),
        ("timezone", timezone),
        (
            "system_version",
            format!("{os} {} {release}", std::env::consts::OS),
        ),
        ("version_code", "1.12.0".into()),
        ("clientversion", "1.12.0".into()),
        ("agentclientversion", "1.12.0".into()),
        ("agentclienttype", "AGENT_CLI".into()),
    ] {
        headers.insert(
            name,
            HeaderValue::from_str(&value).map_err(|_| "invalid host metadata")?,
        );
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    #[test]
    fn auth_session_replacement_validates_cookie_and_revives_client_only_explicitly() {
        let creds = crate::session::Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-old".into()),
        };
        let mut client = super::Client::test_client(creds, "http://127.0.0.1:1").unwrap();
        client.invalidate();
        assert!(client
            .set_session(zeroize::Zeroizing::new("bad;cookie".into()))
            .is_err());
        assert!(client.invalidated);
        client
            .set_session(zeroize::Zeroizing::new("synthetic-new".into()))
            .unwrap();
        assert_eq!(client.credentials().session_id.as_str(), "synthetic-new");
        assert!(!client.invalidated);
    }
    use super::*;
    use crate::cli::WalletCommand;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};

    fn mock(responses: Vec<String>) -> (String, Receiver<String>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|l| l.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (base, rx, thread)
    }

    fn response(data: Value, cookie: Option<&str>) -> String {
        let body = json!({"code":"000000","data":data}).to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
            body.len(),
            cookie
                .map(|c| format!("Set-Cookie: agentSessionId={c}; Path=/; HttpOnly\r\n"))
                .unwrap_or_default(),
            body
        )
    }

    fn credentials() -> Credentials {
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    #[tokio::test]
    async fn address_uses_bodyless_post_first_wallet_only_and_network_names() {
        let raw = json!([{"subWalletList":[{"addresses":[{"binanceChainId":"1","address":"0x52908400098527886e0f7030069857d2e4169ee7"},{"binanceChainId":"CT_501","address":"So11111111111111111111111111111111111111112"}]}]}, {"subWalletList":[{"addresses":[{"binanceChainId":"56","address":"ignore"}]}]}]);
        let networks = json!([{"binanceChainId":"1","name":"Ethereum","simpleName":"ETH"}]);
        let (base, requests, handle) = mock(vec![response(raw, None), response(networks, None)]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let data = client.execute(&WalletCommand::Address).await.unwrap();
        assert_eq!(
            data,
            json!({"addresses":[{"binanceChainId":"1","chainName":"ETH","address":"0x52908400098527886E0F7030069857D2E4169EE7"},{"binanceChainId":"CT_501","chainName":"CT_501","address":"So11111111111111111111111111111111111111112"}]})
        );
        let first = requests.recv().unwrap();
        assert!(first.starts_with(
            "POST /bapi/defi/v1/public/wallet-direct/agent-wallet/mpc-wallet/list HTTP/1.1\r\n"
        ));
        assert!(first.ends_with("\r\n\r\n"));
        assert!(!first.ends_with("{}"));
        assert!(requests.recv().unwrap().starts_with(
            "GET /bapi/defi/v1/public/wallet-direct/mgmt/agent/networks/active HTTP/1.1\r\n"
        ));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn raw_wallet_queries_preserve_backend_data_and_exact_chain_body() {
        let chain_data = json!([{"binanceChainId":"56","name":"BNB Smart Chain","simpleName":"BSC","ignored":"extra"}]);
        let tokens = json!([{"symbol":"RWA","balance":"0","authorityChanges":["retain"],"price":"0.123456789123456789"}]);
        let (base, requests, handle) = mock(vec![
            response(chain_data, None),
            response(json!({"tokenList":tokens.clone()}), None),
            response(json!({"gas":"1.000000000000000001"}), None),
            response(json!({"locked":false}), None),
            response(json!({"claimed":true,"agentId":"test"}), None),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            client.execute(&WalletCommand::Chains).await.unwrap(),
            json!([{"binanceChainId":"56","name":"BNB Smart Chain","simpleName":"BSC"}])
        );
        assert_eq!(
            client
                .execute(&WalletCommand::Balance(crate::portfolio::BalanceArgs {
                    raw: true,
                    ..Default::default()
                }))
                .await
                .unwrap(),
            tokens
        );
        let chain = || crate::cli::Chain {
            binance_chain_id: "56".into(),
        };
        assert_eq!(
            client
                .execute(&WalletCommand::GasPrice(chain()))
                .await
                .unwrap(),
            json!({"gas":"1.000000000000000001"})
        );
        assert_eq!(
            client
                .execute(&WalletCommand::TxLock(chain()))
                .await
                .unwrap(),
            json!({"locked":false})
        );
        assert_eq!(
            client.execute(&WalletCommand::AgentId).await.unwrap(),
            json!({"claimed":true,"agentId":"test"})
        );
        let reqs: Vec<_> = (0..5).map(|_| requests.recv().unwrap()).collect();
        assert!(reqs[1].starts_with(
            "POST /bapi/defi/v1/public/wallet-direct/agent-wallet/mpc-wallet/token/list HTTP/1.1"
        ));
        assert!(reqs[1].ends_with("\r\n\r\n"));
        for (i, suffix) in [(2, "/blockchain/gasPrice"), (3, "/order/tx-lock-status")] {
            assert!(reqs[i].starts_with(&format!("POST {API}{suffix} HTTP/1.1")));
            assert_eq!(
                serde_json::from_str::<Value>(reqs[i].split("\r\n\r\n").nth(1).unwrap()).unwrap(),
                json!({"binanceChainId":"56"})
            );
        }
        assert!(reqs[4].starts_with(
            "GET /bapi/defi/v1/public/wallet-direct/agent-wallet/agent-id/claimed HTTP/1.1"
        ));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn settings_returns_lossless_documented_composite() {
        let settings = json!({"dailyLimit":"1.23456789123456789","authorityChanges":[{"spender":"retain"}],"unknownNewField":true});
        let trading_limit = json!({"quotaUsed":"0","authorityChanges":["quota"]});
        let login = json!({"sessionExpireTime":"1234567890","connectionStatus":"CONNECTED"});
        let (base, requests, handle) = mock(vec![
            response(settings.clone(), None),
            response(trading_limit.clone(), None),
            response(login.clone(), None),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            client.execute(&WalletCommand::Settings).await.unwrap(),
            json!({"settings":settings,"tradingLimit":trading_limit,"login":login})
        );
        for suffix in ["/settings/query", "/trading-limit/query", "/login/query"] {
            assert!(requests
                .recv()
                .unwrap()
                .starts_with(&format!("GET {API}{suffix} HTTP/1.1")));
        }
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn authentication_errors_poison_all_followup_commands_and_never_echo_body() {
        for reply in [
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 16\r\nConnection: close\r\n\r\nsecret-raw-error".to_owned(),
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            { let body=r#"{"code":"100001005","message":"secret-raw-error","data":null}"#; format!("HTTP/1.1 200 OK\r\nSet-Cookie: agentSessionId=must-not-adopt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body) },
        ] {
            let (base,requests,handle)=mock(vec![reply]);
            let mut client=Client::test_client(credentials(),&base).unwrap();
            let error=client.execute(&WalletCommand::Status).await.unwrap_err();
            assert!(!error.contains("secret") && !error.contains("synthetic"));
            requests.recv().unwrap(); handle.join().unwrap();
            assert!(client.credentials.session_id.is_empty());
            let next=client.execute(&WalletCommand::AgentId).await.unwrap_err();
            assert!(next.contains("session invalidated"));
        }
    }

    #[tokio::test]
    async fn rejects_redirects_without_following_location_or_changing_cookie() {
        let sink = TcpListener::bind("127.0.0.1:0").unwrap();
        sink.set_nonblocking(true).unwrap();
        let (base,requests,handle)=mock(vec![format!("HTTP/1.1 302 Found\r\nLocation: http://{}/credential-sink\r\nSet-Cookie: agentSessionId=must-not-adopt\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",sink.local_addr().unwrap())]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            client.execute(&WalletCommand::Status).await.unwrap_err(),
            "HTTP 302"
        );
        assert_eq!(client.credentials.session_id.as_str(), "synthetic-session");
        assert_eq!(
            sink.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn failed_api_and_malformed_json_never_adopt_cookie_or_echo_messages() {
        for body in [
            r#"{"code":"123456","message":"secret-session-value","data":null}"#,
            "secret-session-value",
        ] {
            let (base,requests,handle)=mock(vec![format!("HTTP/1.1 200 OK\r\nSet-Cookie: agentSessionId=must-not-adopt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body)]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            assert!(!client
                .execute(&WalletCommand::Status)
                .await
                .unwrap_err()
                .contains("secret"));
            assert_eq!(client.credentials.session_id.as_str(), "synthetic-session");
            requests.recv().unwrap();
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn responses_are_bounded_and_missing_sessions_never_connect() {
        let (base, requests, handle) = mock(vec![format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_RESPONSE + 1
        )]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert!(client
            .execute(&WalletCommand::Status)
            .await
            .unwrap_err()
            .contains("exceeds"));
        requests.recv().unwrap();
        handle.join().unwrap();
        let mut creds = credentials();
        creds.session_id.clear();
        let mut client = Client::test_client(creds, &base).unwrap();
        assert_eq!(
            client.execute(&WalletCommand::Status).await.unwrap(),
            json!({"status":"UNCONNECTED"})
        );
        assert_eq!(
            client.execute(&WalletCommand::Address).await.unwrap_err(),
            "wallet session is missing"
        );
    }

    #[test]
    fn display_checksum_matches_eip55_vectors() {
        for expected in [
            "0x52908400098527886E0F7030069857D2E4169EE7",
            "0x8617E340B3D01FA5F11F306F4090FD50E238070D",
            "0xde709f2102306220921060314715629080e2fb77",
            "0x27b1fdb04752bbc536007a920d24acb045561c26",
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
        ] {
            assert_eq!(checksum_address(&expected.to_lowercase()), expected);
        }
    }

    #[tokio::test]
    async fn batch_runs_sequentially_rotates_session_and_never_caches_business_results() {
        let (base, requests, handle) = mock(vec![
            response(
                json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
                Some("batch-rotated"),
            ),
            response(json!({"connectionStatus":"NO"}), None),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let lines = b"[\"wallet\",\"status\"]\n[\"batch\"]\n{invalid\n[\"wallet\",\"status\"]\n";
        let mut out = Vec::new();
        let mut err = Vec::new();
        let all_ok = crate::cli::run_batch(
            &mut client,
            std::io::Cursor::new(lines),
            &mut out,
            &mut err,
            true,
        )
        .await
        .unwrap();
        assert!(!all_ok);
        let rows: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[0],
            json!({"success":true,"data":{"status":"CONNECTED"}})
        );
        assert_eq!(rows[1]["success"], false);
        assert_eq!(rows[2]["success"], false);
        assert_eq!(
            rows[3],
            json!({"success":true,"data":{"status":"UNCONNECTED"}})
        );
        assert_eq!(String::from_utf8(err).unwrap().lines().count(), 4);
        requests.recv().unwrap();
        assert!(requests
            .recv()
            .unwrap()
            .contains("agentSessionId=batch-rotated"));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn auth_wrapper_codes_invalidate_in_memory_session() {
        for code in ["10003001", "10003002"] {
            let body = json!({"code":code,"message":"secret-auth-detail","data":null}).to_string();
            let (base, requests, handle) = mock(vec![format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            let error = client.execute(&WalletCommand::Status).await.unwrap_err();
            assert!(!error.contains("secret"));
            assert!(client.credentials.session_id.is_empty());
            assert!(client
                .execute(&WalletCommand::Status)
                .await
                .unwrap_err()
                .contains("invalidated"));
            requests.recv().unwrap();
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn batch_reuses_one_real_tcp_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            for (index, status) in ["CONNECTED", "NO"].into_iter().enumerate() {
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    std::io::BufRead::read_line(&mut reader, &mut line).unwrap();
                    let end = line == "\r\n";
                    request.push_str(&line);
                    if end {
                        break;
                    }
                    assert!(!line.is_empty(), "connection must be reused");
                }
                assert!(request.starts_with(&format!("GET {API}/login/query HTTP/1.1")));
                let body=json!({"code":"000000","data":{"connectionStatus":status,"walletCreateStatus":"CREATED"}}).to_string();
                let connection = if index == 0 { "keep-alive" } else { "close" };
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
            }
        });
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut output = Vec::new();
        assert!(crate::cli::run_batch(
            &mut client,
            std::io::Cursor::new(b"[\"wallet\",\"status\"]\n[\"wallet\",\"status\"]\n"),
            &mut output,
            &mut Vec::new(),
            false
        )
        .await
        .unwrap());
        assert_eq!(String::from_utf8(output).unwrap().lines().count(), 2);
        handle.join().unwrap();
    }

    #[test]
    fn rejects_cookie_delimiters_before_building_requests() {
        for token in [
            "synthetic;OtherCookie=injected",
            "synthetic,second",
            "synthetic\nheader",
        ] {
            let mut creds = credentials();
            creds.session_id = zeroize::Zeroizing::new(token.into());
            assert!(Client::new(creds).is_err());
        }
    }

    #[tokio::test]
    async fn check_cache_reuses_reads_within_ttl_and_never_at_zero_ttl() {
        // Three replies: one for the reused read, two for the two strict reads.
        let settings_one = response(
            json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
            None,
        );
        let settings_two = settings_one.clone();
        let settings_three = settings_one.clone();
        let (base, requests, handle) = mock(vec![settings_one, settings_two, settings_three]);
        let mut client = Client::test_client(credentials(), &base).unwrap();

        let first = client
            .settings_with_ttl(std::time::Duration::from_secs(30))
            .await
            .unwrap();
        let second = client
            .settings_with_ttl(std::time::Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(first, second);
        // A zero TTL must always re-read, even with a populated cache.
        client
            .settings_with_ttl(std::time::Duration::ZERO)
            .await
            .unwrap();
        client
            .settings_with_ttl(std::time::Duration::ZERO)
            .await
            .unwrap();
        handle.join().unwrap();

        let seen: Vec<String> = requests.into_iter().collect();
        assert_eq!(
            seen.len(),
            3,
            "1 cached read + 1 at zero TTL, cache never used"
        );
        assert!(seen.iter().all(|r| r.contains("/settings/query")));
    }

    #[tokio::test]
    async fn clearing_the_check_cache_forces_a_fresh_read() {
        let reply = || {
            response(
                json!({"devMode":{"enabled":true,"expiresAt":4102444800u64}}),
                None,
            )
        };
        let (base, requests, handle) = mock(vec![reply(), reply()]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        client
            .settings_with_ttl(std::time::Duration::from_secs(30))
            .await
            .unwrap();
        client.clear_check_caches();
        client
            .settings_with_ttl(std::time::Duration::from_secs(30))
            .await
            .unwrap();
        handle.join().unwrap();
        assert_eq!(requests.into_iter().count(), 2);
    }

    #[tokio::test]
    async fn native_status_rotates_cookie_only_in_memory_and_sends_exact_headers() {
        let (base, requests, handle) = mock(vec![
            response(
                json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATED"}),
                Some("rotated-synthetic"),
            ),
            response(
                json!({"connectionStatus":"CONNECTED","walletCreateStatus":"CREATING"}),
                None,
            ),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            client.execute(&WalletCommand::Status).await.unwrap(),
            json!({"status":"CONNECTED"})
        );
        assert_eq!(
            client.execute(&WalletCommand::Status).await.unwrap(),
            json!({"status":"CREATING"})
        );
        let first = requests.recv().unwrap().to_lowercase();
        let second = requests.recv().unwrap().to_lowercase();
        assert!(first.starts_with(
            "get /bapi/defi/v1/public/wallet-direct/agent-wallet/login/query http/1.1\r\n"
        ));
        for header in [
            "agentclientid: synthetic-client",
            "bnc-uuid: synthetic-client",
            "cookie: agentsessionid=synthetic-session",
            "version_code: 1.12.0",
            "clientversion: 1.12.0",
            "agentclientversion: 1.12.0",
            "agentclienttype: agent_cli",
            "content-type: application/json",
            "agentclientmac:",
            "agentclientsystem:",
            "device_name:",
            "system_lang:",
            "timezone:",
            "system_version:",
        ] {
            assert!(first.contains(header), "missing {header}");
        }
        assert!(first.contains(concat!("user-agent: baw-rs/", env!("CARGO_PKG_VERSION"))));
        assert!(!first.contains("authorization:"));
        assert!(second.contains("cookie: agentsessionid=rotated-synthetic"));
        let trace = first
            .lines()
            .find_map(|x| x.strip_prefix("x-trace-id: "))
            .unwrap();
        assert_eq!(trace.len(), 32);
        assert!(trace.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(
            trace,
            second
                .lines()
                .find_map(|x| x.strip_prefix("x-trace-id: "))
                .unwrap()
        );
        handle.join().unwrap();
    }
}
