//! Native one-shot EVM log/pending tasks. No shell, eval or implicit mutation retries.
#[path = "native_pending.rs"]
mod pending;
use super::*;
use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Load { config: Value },
    Arm { task_id: String, confirm: bool },
    Status {},
    Disarm { task_id: String },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub task_id: String,
    pub valid_for_ms: u64,
    /// Resolved at the first arm attempt only; never accepted in user config or serialized there.
    #[serde(skip)]
    pub deadline_ms: u64,
    #[serde(skip)]
    pub monotonic_deadline: Option<Instant>,
    #[cfg(test)]
    #[serde(skip)]
    route_test_origin: Option<String>,
    pub chain_id: String,
    pub rpc_url: String,
    pub ws_url: String,
    #[serde(default)]
    pub source: TriggerSource,
    #[serde(default)]
    pub filter: Option<Filter>,
    #[serde(default)]
    pub start_block: u64,
    #[serde(default)]
    pub confirmation_depth: u64,
    #[serde(default)]
    pub max_backfill_blocks: u64,
    #[serde(default)]
    pub chunk_blocks: u64,
    pub queue_capacity: usize,
    pub action: Action,
    pub policy: Value,
    #[serde(default)]
    pub prerequisites: Vec<ReadCheck>,
    /// Routing services return opaque calldata whose recipient, token path and minimum output
    /// cannot be decoded here. Accepting that is an explicit, per-task decision, never a default.
    #[serde(default)]
    pub allow_unbound_route_calldata: bool,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TriggerSource {
    #[default]
    Logs,
    Pending {
        subscription: PendingSubscription,
        from: String,
        to: String,
        selector: String,
        args: Vec<PendingArg>,
    },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PendingSubscription {
    Hashes,
    Full,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingArg {
    index: usize,
    abi_type: String,
    equals: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Filter {
    address: String,
    topics: Vec<Option<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    Fixed { call: Call },
    Words { call: Call, args: Vec<Arg> },
    Route { config: Value },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Arg {
    source: Source,
    abi_type: String,
    max: String,
    #[serde(default)]
    add: Option<String>,
    #[serde(default)]
    multiply: Option<String>,
    #[serde(default)]
    divide: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Source {
    /// Static ABI word in actual pending transaction input, after its four-byte selector.
    Input {
        index: usize,
    },
    Topic {
        index: usize,
    },
    Data {
        index: usize,
    },
    Constant {
        word: String,
    },
    Read {
        to: String,
        input_data: String,
        word_index: usize,
    },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadCheck {
    to: String,
    input_data: String,
    word_index: usize,
    min: String,
}
fn clock() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock invalid")?
        .as_millis() as u64)
}
fn hex(s: &str, bytes: usize) -> bool {
    s.starts_with("0x") && s.len() == 2 + bytes * 2 && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn data(s: &str) -> bool {
    s.starts_with("0x")
        && s.len() <= 32770
        && s.len().is_multiple_of(2)
        && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn quantity(v: &Value) -> Result<u64, String> {
    u64::from_str_radix(
        v.as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .filter(|s| !s.is_empty())
            .ok_or("malformed RPC quantity")?,
        16,
    )
    .map_err(|_| "RPC quantity overflow".into())
}
fn endpoint(s: &str, ws: bool) -> Result<(), String> {
    let u = url::Url::parse(s).map_err(|_| "invalid RPC endpoint")?;
    let secure = if ws { "wss" } else { "https" };
    let local = matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || (u.scheme() != secure && !(local && u.scheme() == if ws { "ws" } else { "http" }))
    {
        return Err(
            "RPC endpoints require TLS (plaintext loopback fixtures only), no userinfo/fragment"
                .into(),
        );
    }
    Ok(())
}
pub(super) fn word(s: &str, index: usize) -> Result<String, String> {
    // ABI blobs are whole 32-byte words: a trailing partial word means the source data is not
    // canonical, so refuse it instead of silently dropping the stray bytes.
    if !data(s) || !(s.len() - 2).is_multiple_of(64) || index >= 256 {
        return Err("invalid or excessive ABI words".into());
    }
    let start = 2 + index * 64;
    s.get(start..start + 64)
        .map(|s| format!("0x{s}"))
        .ok_or("ABI word missing".into())
}
fn validate_duration(ms: u64) -> Result<(), String> {
    if ms == 0
        || clock()?.checked_add(ms).is_none()
        || tokio::time::Instant::now()
            .checked_add(Duration::from_millis(ms))
            .is_none()
    {
        return Err(
            "valid_for_ms must be a positive finite representable duration (no overflow)".into(),
        );
    }
    Ok(())
}
impl Config {
    pub(super) fn parse(v: Value) -> Result<Self, String> {
        if v.get("deadline_ms").is_some() {
            return Err("native task requires relative valid_for_ms; absolute deadline_ms is unsupported (including mixed configs)".into());
        }
        let explicit_log_bounds = [
            "start_block",
            "confirmation_depth",
            "max_backfill_blocks",
            "chunk_blocks",
        ]
        .iter()
        .all(|key| v.get(key).is_some());
        let c: Self = serde_json::from_value(v)
            .map_err(|e| format!("unsupported native task configuration: {e}"))?;
        crate::execution::require_consent(true, Some(&c.task_id))?;
        validate_duration(c.valid_for_ms)?;
        if c.valid_for_ms == 0 || c.queue_capacity == 0 || c.queue_capacity > 256 {
            return Err("native task deadline/resource bounds exceeded".into());
        }
        if c.chain_id.is_empty() || !c.chain_id.bytes().all(|x| x.is_ascii_digit()) {
            return Err("native task requires decimal EVM chain".into());
        }
        endpoint(&c.rpc_url, false)?;
        endpoint(&c.ws_url, true)?;
        match &c.source {
            TriggerSource::Logs => {
                if !explicit_log_bounds {
                    return Err("log source requires explicit start_block/confirmation_depth/max_backfill_blocks/chunk_blocks".into());
                }
                let f = c.filter.as_ref().ok_or("log filter required")?;
                if c.chunk_blocks == 0
                    || c.chunk_blocks > 1000
                    || c.max_backfill_blocks == 0
                    || c.max_backfill_blocks > 10000
                    || c.confirmation_depth > 256
                {
                    return Err("native log resource bounds exceeded".into());
                }
                if !hex(&f.address, 20)
                    || f.topics.is_empty()
                    || f.topics.len() > 4
                    || f.topics.iter().all(Option::is_none)
                    || f.topics.iter().flatten().any(|s| !hex(s, 32))
                {
                    return Err("explicit address and nonempty pinned topics required".into());
                }
            }
            TriggerSource::Pending {
                from,
                to,
                selector,
                args,
                ..
            } => {
                if c.filter.is_some()
                    || c.start_block != 0
                    || c.confirmation_depth != 0
                    || c.max_backfill_blocks != 0
                    || c.chunk_blocks != 0
                {
                    return Err(
                        "pending source does not support log filter/backfill/confirmations".into(),
                    );
                }
                if !hex(from, 20) || !hex(to, 20) || !hex(selector, 4) || args.len() > 64 {
                    return Err(
                        "pending source requires pinned from/to/selector and bounded ABI args"
                            .into(),
                    );
                }
                let mut indices = std::collections::HashSet::new();
                for a in args {
                    if a.index >= 256 || !indices.insert(a.index) || !hex(&a.equals, 32) {
                        return Err("pending ABI argument index/word invalid or repeated".into());
                    }
                    let n = crate::uint::uint256(&a.equals)?;
                    match a.abi_type.as_str() {
                        "uint256" | "bytes32" => {},
                        "address" if n.bits() <= 160 => {},
                        "bool" if n <= num_bigint::BigUint::from(1u8) => {},
                        _ => return Err("unsupported/noncanonical pending ABI word (uint256/address/bytes32/bool only)".into()),
                    }
                }
            }
        }
        let p = crate::policy::Policy::parse(&c.policy.to_string())?;
        if p.max_total_wei.is_some() {
            return Err("native maxTotalWei unsupported".into());
        }
        match &c.action {
            Action::Fixed { call } | Action::Words { call, .. } => {
                c.validate_call(call)?;
                if let Action::Words { args, .. } = &c.action {
                    if call.input_data.len() != 10 || args.is_empty() || args.len() > 64 {
                        return Err(
                            "words supports selector plus 1..64 static ABI words only".into()
                        );
                    }
                    for a in args {
                        if !matches!(
                            a.abi_type.as_str(),
                            "uint256" | "address" | "bytes32" | "bool"
                        ) {
                            return Err(
                                "unsupported ABI type; only uint256/address/bytes32/bool words"
                                    .into(),
                            );
                        }
                        crate::uint::uint256(&a.max)?;
                        for x in [&a.add, &a.multiply, &a.divide].into_iter().flatten() {
                            crate::uint::uint256(x)?;
                        }
                        if a.divide.as_ref().is_some_and(|s| {
                            crate::uint::uint256(s)
                                .is_ok_and(|x| x == num_bigint::BigUint::from(0u8))
                        }) {
                            return Err("division by zero".into());
                        }
                        let pending = matches!(c.source, TriggerSource::Pending { .. });
                        if (pending
                            && matches!(
                                a.source,
                                Source::Topic { .. } | Source::Data { .. } | Source::Read { .. }
                            ))
                            || (!pending && matches!(a.source, Source::Input { .. }))
                        {
                            return Err("action source incompatible with trigger: pending words allow only input/constant; logs do not define input".into());
                        }
                        match &a.source {
                            Source::Topic { index } if *index > 3 => {
                                return Err("topic index exceeds 3".into())
                            }
                            Source::Input { index }
                            | Source::Data { index }
                            | Source::Read {
                                word_index: index, ..
                            } if *index >= 256 => return Err("word index exceeds limit".into()),
                            Source::Constant { word: w } if !hex(w, 32) => {
                                return Err("constant must be exact ABI word".into())
                            }
                            _ => {}
                        }
                        if let Source::Read { to, input_data, .. } = &a.source {
                            validate_read(to, input_data)?;
                        }
                    }
                }
            }
            Action::Route { config } => {
                native_route_builder(config, 1)?; // Validation-only sentinel, never an authorization.
            }
        }
        if c.prerequisites.len() > 16 {
            return Err("too many prerequisites".into());
        }
        for r in &c.prerequisites {
            validate_read(&r.to, &r.input_data)?;
            crate::uint::uint256(&r.min)?;
            if r.word_index >= 256 {
                return Err("prerequisite word out of range".into());
            }
        }
        Ok(c)
    }
    fn validate_call(&self, call: &Call) -> Result<(), String> {
        call.validate()?;
        if call.binance_chain_id != self.chain_id {
            return Err("call chain differs from event chain".into());
        }
        let p = crate::policy::Policy::parse(&self.policy.to_string())?;
        p.check_intent(&crate::policy::Intent {
            chain_id: &call.binance_chain_id,
            to: &call.to,
            value: &call.value,
            input_data: Some(&call.input_data),
            gas_limit: call.gas_limit,
        })
    }
    /// Whether the returned route may be submitted at all. Undecodable calldata leaves recipient,
    /// token path and on-chain minimum unbound; accepting that is an explicit per-task decision and
    /// never the default, while a route claiming full validation must actually carry both proofs.
    pub(super) fn route_gate(&self, note: &Value) -> Result<(), String> {
        if !note.is_object()
            || note
                .get("requires_runtime_preview")
                .and_then(Value::as_bool)
                .is_none()
        {
            return Err("route validation metadata malformed".into());
        }
        let unbound = note
            .get("requires_runtime_preview")
            .and_then(Value::as_bool)
            == Some(true);
        if !unbound
            && (note.get("recipient_verified").and_then(Value::as_bool) != Some(true)
                || note
                    .get("calldata_min_out_verified")
                    .and_then(Value::as_bool)
                    != Some(true))
        {
            return Err(
                "route claims full validation without both recipient and minimum proof".into(),
            );
        }
        if unbound && !self.allow_unbound_route_calldata {
            return Err(
                "route calldata is opaque (recipient/minimum unverified); set allow_unbound_route_calldata to accept this explicitly"
                    .into(),
            );
        }
        Ok(())
    }
    pub(super) async fn build(&self, event: &Value) -> Result<(Call, Value), String> {
        // Route actions are resolved only here, i.e. after the triggering event has been observed.
        // Nothing about a route is fetched, cached or reused before that point.
        if let Action::Route { config } = &self.action {
            let monotonic = self
                .monotonic_deadline
                .filter(|t| Instant::now() < *t)
                .ok_or("native route deadline unresolved/expired")?;
            if self.deadline_ms == 0 || clock()? >= self.deadline_ms {
                return Err("native route deadline unresolved/expired".into());
            }
            // Use precisely the wall and monotonic deadlines captured before first-arm readiness.
            let builder = native_route_builder(config, self.deadline_ms)?;
            #[cfg(test)]
            let builder = if let Some(origin) = &self.route_test_origin {
                builder.with_test_origin(origin)?
            } else {
                builder
            };
            let mut out = tokio::time::timeout_at(
                tokio::time::Instant::from_std(monotonic),
                builder.build(event),
            )
            .await
            .map_err(|_| "native route construction deadline expired")??;
            let note = out
                .get("route_validation")
                .cloned()
                .ok_or("route builder returned no validation metadata")?;
            self.route_gate(&note)?;
            out.as_object_mut()
                .ok_or("route call must be an object")?
                .remove("route_validation");
            let call: Call =
                serde_json::from_value(out).map_err(|_| "route call fields invalid")?;
            self.validate_call(&call)?;
            return Ok((call, note));
        }
        let mut call = match &self.action {
            Action::Fixed { call } | Action::Words { call, .. } => call.clone(),
            Action::Route { .. } => unreachable!("route handled above"),
        };
        if let Action::Words { args, .. } = &self.action {
            for a in args {
                let w = match &a.source {
                    Source::Input { index } => {
                        let input = event["input"]
                            .as_str()
                            .filter(|s| data(s) && s.len() >= 10)
                            .ok_or("pending input missing/malformed")?;
                        word(&format!("0x{}", &input[10..]), *index)?
                    }
                    Source::Topic { index } => event["topics"]
                        .get(*index)
                        .and_then(Value::as_str)
                        .filter(|s| hex(s, 32))
                        .ok_or("topic ABI word missing")?
                        .to_owned(),
                    Source::Data { index } => {
                        word(event["data"].as_str().ok_or("event data missing")?, *index)?
                    }
                    Source::Constant { word } => word.clone(),
                    Source::Read {
                        to,
                        input_data,
                        word_index,
                    } => {
                        let result=rpc(self,"eth_call",json!([{"to":to,"data":input_data},{"blockHash":event["blockHash"],"requireCanonical":true}])).await?;
                        word(
                            result.as_str().ok_or("eth_call must return hex")?,
                            *word_index,
                        )?
                    }
                };
                let mut n = crate::uint::uint256(&w)?;
                if let Some(v) = &a.multiply {
                    n *= crate::uint::uint256(v)?;
                }
                if let Some(v) = &a.divide {
                    n /= crate::uint::uint256(v)?;
                }
                if let Some(v) = &a.add {
                    n += crate::uint::uint256(v)?;
                }
                if n.bits() > 256 || n > crate::uint::uint256(&a.max)? {
                    return Err("ABI value exceeds authorized bound".into());
                }
                match a.abi_type.as_str() {
                    "address" if n.bits() > 160 => return Err("invalid ABI address padding".into()),
                    "bool" if n > num_bigint::BigUint::from(1u8) => {
                        return Err("invalid ABI bool".into())
                    }
                    _ => {}
                }
                call.input_data
                    .push_str(&format!("{:0>64}", n.to_str_radix(16)));
            }
        }
        self.validate_call(&call)?;
        Ok((
            call,
            json!({"requires_runtime_preview":false,"recipient_verified":true,"calldata_min_out_verified":true,"kind":"static_task"}),
        ))
    }
}
/// Native tasks expose duration only. The standalone route API intentionally keeps its
/// existing absolute-deadline contract; inject its internal budget into a temporary clone.
fn native_route_builder(
    config: &Value,
    deadline: u64,
) -> Result<crate::route_action::RouteActionBuilder, String> {
    let mut internal = config.clone();
    let fields = internal
        .as_object_mut()
        .ok_or("native route config must be an object")?;
    if fields.contains_key("deadline_unix_ms") {
        return Err("native route requires task valid_for_ms; nested absolute deadline_unix_ms is unsupported (including mixed configs)".into());
    }
    fields.insert("deadline_unix_ms".into(), json!(deadline));
    crate::route_action::RouteActionBuilder::new(&internal)
}
fn validate_read(to: &str, input: &str) -> Result<(), String> {
    if !hex(to, 20) || !data(input) || input.len() < 10 {
        Err("invalid fixed read target/calldata".into())
    } else {
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Book {
    loaded: bool,
    current: Option<Record>,
    history: BTreeMap<String, String>,
    legacy_records: BTreeMap<String, Value>,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
    worker: Option<tokio::task::JoinHandle<()>>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    config: Config,
    resolved_deadline_ms: Option<u64>,
    status: String,
    operation_id: Option<String>,
    detail: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disk {
    current: Option<Record>,
    history: BTreeMap<String, String>,
    /// Immutable baseline records, retained even after a new task replaces current.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    legacy_records: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDisk {
    current: Option<LegacyRecord>,
    history: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecord {
    config: Value,
    status: String,
    operation_id: Option<String>,
    detail: Value,
}
impl LegacyDisk {
    /// Disk-only migration, deliberately separate from Config::parse's user authorization.
    /// Zero duration is a tombstone, not a fresh or inferred authorization window.
    fn migrate(self) -> Result<Disk, String> {
        let mut disk = Disk {
            current: None,
            history: self.history,
            legacy_records: BTreeMap::new(),
        };
        if let Some(old) = self.current {
            let snapshot =
                serde_json::to_value(&old).map_err(|_| "legacy record encoding failed")?;
            let mut v = old.config;
            let fields = v.as_object_mut().ok_or("invalid legacy config")?;
            if fields.contains_key("valid_for_ms") || fields.contains_key("source") {
                return Err("mixed or unsupported legacy state schema".into());
            }
            for key in [
                "filter",
                "start_block",
                "confirmation_depth",
                "max_backfill_blocks",
                "chunk_blocks",
            ] {
                if !fields.contains_key(key) {
                    return Err("incomplete legacy state schema".into());
                }
            }
            let deadline = fields
                .remove("deadline_ms")
                .and_then(|v| v.as_u64())
                .ok_or("invalid legacy deadline")?;
            fields.insert("valid_for_ms".into(), json!(0));
            let config: Config =
                serde_json::from_value(v).map_err(|_| "invalid legacy configuration schema")?;
            let status = match old.status.as_str() {
                // Baseline did not journal EXECUTING before execute: ACCEPTED is ambiguous.
                "ACCEPTED" | "EXECUTING" => "UNKNOWN",
                "LOADED" | "PREPARING" | "CONNECTING" | "READY" => "DISARMED",
                "SUCCESS" | "FAILED" | "UNKNOWN" | "DISARMED" => &old.status,
                _ => return Err("unknown legacy status; reconciliation required".into()),
            }
            .to_owned();
            disk.legacy_records.insert(config.task_id.clone(), snapshot);
            disk.current = Some(Record {
                config,
                resolved_deadline_ms: Some(deadline),
                status,
                operation_id: old.operation_id,
                detail: old.detail,
            });
        }
        Ok(disk)
    }
}
impl Book {
    #[cfg(test)]
    pub(super) fn route_test_origin(&mut self, origin: String) {
        self.current.as_mut().unwrap().config.route_test_origin = Some(origin);
    }
    pub(super) fn restore(&mut self, dir: &Path) -> Result<(), String> {
        if self.loaded {
            return Ok(());
        }
        let path = dir.join("native-event-state.json");
        if std::fs::symlink_metadata(&path).is_ok() {
            use std::io::Read;
            let mut bytes = Vec::new();
            crate::wallets::private_file(&path, false)?
                .take(262145)
                .read_to_end(&mut bytes)
                .map_err(|_| "native state read failed")?;
            if bytes.len() > 262144 {
                return Err("native state too large".into());
            }
            let (d, migrated) = match serde_json::from_slice::<Disk>(&bytes) {
                Ok(d) => (d, false),
                Err(_) => {
                    let old: LegacyDisk = serde_json::from_slice(&bytes)
                        .map_err(|_| "native state corrupt; reconciliation required")?;
                    (old.migrate()?, true)
                }
            };
            // Do not expose partially restored state or skip a failed migration on retry.
            let mut restored = Book {
                current: d.current,
                history: d.history,
                legacy_records: d.legacy_records,
                ..Book::default()
            };
            if let Some(r) = &mut restored.current {
                if r.status == "EXECUTING" {
                    // An execute was in flight when the process died: genuinely unknown.
                    r.status = "UNKNOWN".into();
                } else if r.status == "ACCEPTED" {
                    // The event was accepted but no execute had been submitted yet.
                    r.status = "FAILED".into();
                    r.detail = serde_json::json!({"interrupted":true,"submission_attempted":false,
                                                  "reason":"process restarted before submission"});
                } else if matches!(r.status.as_str(), "READY" | "CONNECTING" | "PREPARING") {
                    r.status = "DISARMED".into();
                }
                restored
                    .history
                    .insert(r.config.task_id.clone(), r.status.clone());
            }
            if migrated {
                // Migration must preserve every byte of semantic evidence, never truncate it
                // to squeeze the expanded archive under the on-disk read cap.
                restored.persist_snapshot(dir, true)?;
            }
            self.current = restored.current;
            self.history = restored.history;
            self.legacy_records = restored.legacy_records;
        }
        self.loaded = true;
        Ok(())
    }
    fn persist(&self, dir: &Path) -> Result<(), String> {
        self.persist_snapshot(dir, false)
    }
    fn persist_snapshot(&self, dir: &Path, preserve_evidence: bool) -> Result<(), String> {
        crate::wallets::private_dir(dir)?;
        let path = dir.join("native-event-state.json");
        if std::fs::symlink_metadata(&path).is_ok() {
            crate::wallets::private_file(&path, false)?;
        }
        let temp = dir.join(format!(".native-event-{:x}", rand::random::<u64>()));
        let mut disk = Disk {
            current: self.current.clone(),
            history: self.history.clone(),
            legacy_records: self.legacy_records.clone(),
        };
        let mut bytes = serde_json::to_vec(&disk).map_err(|_| "state serialization failed")?;
        if bytes.len() > 262144 {
            if preserve_evidence {
                return Err("legacy migration exceeds readable 256KiB cap; original state retained for reconciliation".into());
            }
            // The reader caps at 256 KiB, so persisting more would leave a file that can never be
            // restored. Keep the state machine and drop the bulky evidence instead of bricking it.
            if let Some(r) = &mut disk.current {
                r.detail = json!({"truncated":true,"reason":"state exceeded 256KiB; evidence dropped",
                                  "status":r.status,"operation_id":r.operation_id});
            }
            bytes = serde_json::to_vec(&disk).map_err(|_| "state serialization failed")?;
            if bytes.len() > 262144 {
                return Err("native state exceeds the readable 256KiB cap".into());
            }
        }
        write_temp(&temp, &bytes)?;
        if std::fs::rename(&temp, &path).is_err() {
            // The rename failure path owns the temp file just as much as a write failure does.
            let _ = std::fs::remove_file(&temp);
            return Err("native state rename failed".into());
        }
        File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "native state directory sync failed".into())
    }
    pub(super) fn status(&self) -> Value {
        self.current.as_ref().map(|r|json!({"task_id":r.config.task_id,"status":r.status,"operation_id":r.operation_id,"detail":r.detail,"one_shot":true,"retry_policy":"never","confirmation_depth":r.config.confirmation_depth,"source":r.config.source,"valid_for_ms":r.config.valid_for_ms,"resolved_deadline_ms":r.resolved_deadline_ms})).unwrap_or_else(||json!({"status":"EMPTY"}))
    }
}
/// Write the serialized state into a fresh owner-only temp file and fsync it.
fn write_temp(temp: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    write_temp_with(temp, bytes, |f, b| {
        f.write_all(b).and_then(|_| f.sync_all())
    })
}
/// Create/write/sync a temp file. `write` is a seam: nothing observable yet for a root-owned
/// directory fails after a successful O_CREAT|O_EXCL, so the storage fault is injected there.
pub(super) fn write_temp_with(
    temp: &Path,
    bytes: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let opened = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(temp);
    let mut f = match opened {
        Ok(f) => f,
        // Nothing was created here, so there is nothing to unlink.
        Err(_) => return Err("native state create failed".into()),
    };
    let written = write(&mut f, bytes);
    drop(f);
    if written.is_err() {
        // A leftover 0600 temp file would accumulate on every repeated disk fault.
        let _ = std::fs::remove_file(temp);
        return Err("native state sync failed".into());
    }
    Ok(())
}
impl Wallet {
    pub(super) async fn native_stop(&self) {
        self.native_stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let b = self.native.lock().await;
        if let Some(c) = &b.cancel {
            c.send_replace(true);
        }
    }
    pub(super) async fn native_drain(&self) {
        let task = self.native.lock().await.worker.take();
        if let Some(t) = task {
            let _ = t.await;
        }
    }
    /// Synchronous last-gate journal marker: the next step may submit execute, so a crash
    /// must not later report this accepted operation as provably unsent. Never retries.
    pub(super) fn native_mark_submitting(&self, operation_id: &str) -> Result<(), String> {
        let mut b = self
            .native
            .try_lock()
            .map_err(|_| "native state busy before submission")?;
        let r = b
            .current
            .as_mut()
            .filter(|r| r.status == "ACCEPTED" && r.operation_id.as_deref() == Some(operation_id))
            .ok_or("native operation changed before submission")?;
        r.status = "EXECUTING".into();
        let task_id = r.config.task_id.clone();
        b.history.insert(task_id, "EXECUTING".into());
        b.persist(&self.entry.dir)
    }
    async fn native_finish(
        &self,
        task_id: &str,
        status: &str,
        detail: Value,
    ) -> Result<(), String> {
        let mut b = self.native.lock().await;
        let r = b.current.as_mut().ok_or("native task missing")?;
        // A worker from an earlier task must never write into a newly loaded record.
        if r.config.task_id != task_id {
            return Err("native record changed; refusing stale worker write".into());
        }
        r.status = status.into();
        r.detail = detail;
        let id = r.config.task_id.clone();
        b.history.insert(id, status.into());
        let result = b.persist(&self.entry.dir);
        self.native_reserved
            .store(false, std::sync::atomic::Ordering::SeqCst);
        result
    }
    pub(super) async fn native_control(
        self: &Arc<Self>,
        control: Control,
    ) -> Result<Value, String> {
        match control {
            Control::Arm { task_id, confirm } => {
                if self
                    .native_stopping
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    return Err("resident stopping; no new native task".into());
                }
                crate::execution::require_consent(confirm, Some(&task_id))?;
                let config = {
                    let mut b = self.native.lock().await;
                    b.restore(&self.entry.dir)?;
                    let r = b
                        .current
                        .as_mut()
                        .filter(|r| r.config.task_id == task_id && r.status == "LOADED")
                        .ok_or("native task not LOADED; no replay")?;
                    if r.resolved_deadline_ms.is_some() {
                        return Err(
                            "native task already activated; duration cannot be renewed".into()
                        );
                    }
                    validate_duration(r.config.valid_for_ms)?;
                    let deadline = clock()?
                        .checked_add(r.config.valid_for_ms)
                        .ok_or("native valid_for_ms deadline overflow")?;
                    let monotonic = Instant::now()
                        .checked_add(Duration::from_millis(r.config.valid_for_ms))
                        .ok_or("native valid_for_ms monotonic deadline overflow")?;
                    if self
                        .native_reserved
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err("BUSY; native wallet already reserved".into());
                    }
                    r.status = "PREPARING".into();
                    r.resolved_deadline_ms = Some(deadline);
                    let mut c = r.config.clone();
                    c.deadline_ms = deadline;
                    c.monotonic_deadline = Some(monotonic);
                    if let Err(e) = b.persist(&self.entry.dir) {
                        // A disk fault here would otherwise reserve the wallet with no owner.
                        self.native_reserved
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        return Err(e);
                    }
                    c
                };
                let ready = async {
                    let from = match &config.action {
                        Action::Fixed { call } | Action::Words { call, .. } => call.from.as_str(),
                        Action::Route { config } => config["user_wallet_address"]
                            .as_str()
                            .ok_or("route sender missing")?,
                    };
                    let binding = self.native_readiness(&config.chain_id, from).await?;
                    let chain = rpc(&config, "eth_chainId", json!([])).await?;
                    if crate::uint::uint256(chain.as_str().ok_or("chainId missing")?)?
                        != crate::uint::uint256(&config.chain_id)?
                    {
                        return Err("RPC chainId mismatch".into());
                    }
                    for check in &config.prerequisites {
                        let result = rpc(
                            &config,
                            "eth_call",
                            json!([{"to":check.to,"data":check.input_data},"latest"]),
                        )
                        .await?;
                        let n = crate::uint::uint256(&word(
                            result.as_str().ok_or("prerequisite result missing")?,
                            check.word_index,
                        )?)?;
                        if n < crate::uint::uint256(&check.min)? {
                            return Err("native prerequisite below minimum".into());
                        }
                    }
                    Ok::<_, String>(binding)
                }
                .await;
                let binding = match ready {
                    Ok(b) => b,
                    Err(e) => {
                        self.native_finish(
                            &config.task_id,
                            "FAILED",
                            json!({"error":e,"submission_attempted":false}),
                        )
                        .await?;
                        return Err(e);
                    }
                };
                let mut b = self.native.lock().await;
                let r = b.current.as_mut().ok_or("native task missing")?;
                // Evaluate the clock without `?`: unwinding here would leave the reservation held.
                if r.config.task_id != config.task_id {
                    return Err("native record changed; refusing stale readiness completion".into());
                }
                let expired = clock().map(|t| t >= config.deadline_ms).unwrap_or(true)
                    || config
                        .monotonic_deadline
                        .is_none_or(|t| Instant::now() >= t);
                if r.status != "PREPARING"
                    || self
                        .native_stopping
                        .load(std::sync::atomic::Ordering::SeqCst)
                    || expired
                {
                    if r.status == "PREPARING" {
                        r.status = if expired { "FAILED" } else { "DISARMED" }.into();
                        r.detail = json!({"error":"native task disarmed/expired during readiness","submission_attempted":false});
                    }
                    let status = r.status.clone();
                    b.history.insert(config.task_id.clone(), status);
                    self.native_reserved
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    b.persist(&self.entry.dir)?;
                    return Err("native task disarmed/expired during readiness".into());
                }
                r.status = "CONNECTING".into();
                let (cancel, rx) = tokio::sync::watch::channel(false);
                b.cancel = Some(cancel);
                if let Err(e) = b.persist(&self.entry.dir) {
                    // No worker exists yet, so nothing would ever release the wallet.
                    self.native_reserved
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return Err(e);
                }
                let w = self.clone();
                b.worker = Some(tokio::spawn(async move {
                    w.native_run(config, binding, rx).await;
                }));
                Ok(b.status())
            }
            Control::Load { config } => {
                let config = Config::parse(config)?;
                let mut b = self.native.lock().await;
                b.restore(&self.entry.dir)?;
                if b.worker.as_ref().is_some_and(|w| !w.is_finished()) {
                    return Err(
                        "previous native worker draining; wait before loading a new task".into(),
                    );
                }
                if b.history.contains_key(&config.task_id) {
                    return Err("native task id already used; cannot reload/replay".into());
                }
                if b.current.as_ref().is_some_and(|r| {
                    matches!(
                        r.status.as_str(),
                        "LOADED" | "READY" | "CONNECTING" | "PREPARING" | "ACCEPTED" | "EXECUTING"
                    )
                }) {
                    return Err("native task already loaded; disarm first".into());
                }
                if b.history.len() >= 256 {
                    return Err("native task history full; reconcile before maintenance".into());
                }
                b.history.insert(config.task_id.clone(), "LOADED".into());
                b.current = Some(Record {
                    config,
                    resolved_deadline_ms: None,
                    status: "LOADED".into(),
                    operation_id: None,
                    detail: Value::Null,
                });
                b.persist(&self.entry.dir)?;
                Ok(b.status())
            }
            Control::Status {} => {
                let mut b = self.native.lock().await;
                b.restore(&self.entry.dir)?;
                Ok(b.status())
            }
            Control::Disarm { task_id } => {
                let mut b = self.native.lock().await;
                b.restore(&self.entry.dir)?;
                let r = b
                    .current
                    .as_mut()
                    .filter(|r| r.config.task_id == task_id)
                    .ok_or("unknown native task")?;
                if matches!(r.status.as_str(), "ACCEPTED" | "EXECUTING") {
                    return Err("accepted operation draining; cannot cancel mutation".into());
                }
                if matches!(
                    r.status.as_str(),
                    "LOADED" | "READY" | "CONNECTING" | "PREPARING"
                ) {
                    r.status = "DISARMED".into();
                }
                let status = r.status.clone();
                b.history.insert(task_id, status);
                if let Some(c) = b.cancel.take() {
                    c.send_replace(true);
                }
                // Release first: a failed persist must not leave the wallet wedged as BUSY.
                self.native_reserved
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                b.persist(&self.entry.dir)?;
                Ok(b.status())
            }
        }
    }
}
#[derive(Clone)]
enum Health {
    Connecting,
    Ready,
    Fault(String),
}
pub(super) struct Event {
    value: Value,
    received: Instant,
    origin: &'static str,
}
/// Bounded sliding window of recently observed ids, shared by log and pending dedup. Evicting the
/// oldest id can allow a fresh look at it later; one-shot execution is enforced by the operation
/// state machine, so this window only bounds memory and never limits the task lifetime.
#[derive(Default)]
pub(super) struct RecentIds {
    seen: std::collections::HashSet<String>,
    order: std::collections::VecDeque<String>,
}
impl RecentIds {
    /// False when the id is still inside the window, i.e. already queued.
    pub(super) fn observe(&mut self, id: String) -> bool {
        if self.seen.contains(&id) {
            return false;
        }
        if self.seen.len() >= 4096 {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        self.seen.insert(id.clone());
        self.order.push_back(id);
        true
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.order.len()
    }
}
pub(super) fn event_id(c: &Config, e: &Value) -> Result<String, String> {
    if matches!(c.source, TriggerSource::Pending { .. }) {
        return pending::id(c, e)?
            .ok_or("pending transaction does not match authorized trigger".into());
    }
    // A reorg-marked log is never a trigger. A missing flag is treated as a live log because some
    // nodes omit `removed` in eth_getLogs responses; anything non-boolean still fails closed.
    match e.get("removed") {
        None => {}
        Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => {
            return Err("removed/malformed event; reconciliation required".into());
        }
        Some(_) => return Err("removed/malformed event; reconciliation required".into()),
    }
    let filter = c.filter.as_ref().ok_or("log filter required")?;
    if !e["address"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(&filter.address))
    {
        return Err("log address filter mismatch".into());
    }
    let topics = e["topics"].as_array().ok_or("log topics missing")?;
    if topics.len() > 4
        || topics
            .iter()
            .any(|t| !t.as_str().is_some_and(|s| hex(s, 32)))
    {
        return Err("malformed log topics".into());
    }
    for (i, want) in filter.topics.iter().enumerate() {
        if let Some(want) = want {
            if !topics
                .get(i)
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case(want))
            {
                return Err("log topic filter mismatch".into());
            }
        }
    }
    if !e["data"].as_str().is_some_and(data) {
        return Err("malformed log data".into());
    }
    for key in ["blockHash", "transactionHash"] {
        if !e[key].as_str().is_some_and(|s| hex(s, 32)) {
            return Err("log identity missing/malformed".into());
        }
    }
    let index = quantity(&e["logIndex"])?;
    let block = quantity(&e["blockNumber"])?;
    if block < c.start_block {
        return Err("event before authorized start block".into());
    }
    let mut h = Sha256::new();
    for s in [
        &c.task_id,
        &c.chain_id,
        e["blockHash"].as_str().unwrap(),
        e["transactionHash"].as_str().unwrap(),
        &index.to_string(),
    ] {
        h.update(s.to_ascii_lowercase().as_bytes());
        h.update([0]);
    }
    // 4 prefix chars + 60 hex digits = exactly the 64-char operation-id limit; truncate explicitly.
    let hex = format!("{:x}", h.finalize());
    Ok(format!("evt-{}", &hex[..60]))
}
pub(super) fn enqueue(
    c: &Config,
    value: Value,
    origin: &'static str,
    tx: &tokio::sync::mpsc::Sender<Event>,
    seen: &mut RecentIds,
) -> Result<(), String> {
    let received = Instant::now();
    let id = event_id(c, &value)?;
    // Dedup is a bounded memory concern only: a full window evicts the oldest id instead of
    // ending a task that is authorized to run for hours or days.
    if !seen.observe(id) {
        return Ok(());
    }
    tx.try_send(Event {
        value,
        received,
        origin,
    })
    .map_err(|_| "event queue overflow/closed; not READY".into())
}
async fn backfill(
    c: &Config,
    cursor: &mut u64,
    tx: &tokio::sync::mpsc::Sender<Event>,
    seen: &mut RecentIds,
) -> Result<(), String> {
    let filter = c.filter.as_ref().ok_or("log filter required")?;
    let head = quantity(&rpc(c, "eth_blockNumber", json!([])).await?)?;
    let Some(end) = head.checked_sub(c.confirmation_depth) else {
        return Ok(());
    };
    if end < *cursor {
        return Ok(());
    }
    if end - *cursor >= c.max_backfill_blocks {
        return Err("backfill gap exceeds authorized bound; not READY".into());
    }
    while *cursor <= end {
        let last = cursor.saturating_add(c.chunk_blocks - 1).min(end);
        let filter = json!({"address":filter.address,"topics":filter.topics,"fromBlock":format!("0x{:x}",cursor),"toBlock":format!("0x{last:x}")});
        let rows = rpc(c, "eth_getLogs", json!([filter])).await?;
        let rows = rows.as_array().ok_or("getLogs result is not an array")?;
        if rows.len() > 4096 {
            return Err("getLogs row resource limit".into());
        }
        let mut previous = None;
        for e in rows {
            let b = quantity(&e["blockNumber"])?;
            let i = quantity(&e["logIndex"])?;
            if b < *cursor || b > last || previous.is_some_and(|p| (b, i) < p) {
                return Err("getLogs unordered/out-of-range result".into());
            }
            previous = Some((b, i));
            enqueue(c, e.clone(), "backfill", tx, seen)?;
        }
        *cursor = last.checked_add(1).ok_or("block cursor overflow")?;
    }
    Ok(())
}
async fn source(
    wallet: Arc<Wallet>,
    c: Config,
    tx: tokio::sync::mpsc::Sender<Event>,
    health: tokio::sync::watch::Sender<Health>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) {
    let result=async {
        if matches!(c.source, TriggerSource::Pending { .. }) {
            return pending::run(&wallet, &c, &tx, &health, &mut cancel).await;
        }
        let filter = c.filter.as_ref().ok_or("log filter required")?;
        let mut cursor=c.start_block;let mut seen=RecentIds::default();let mut reconnects=0u32;
        loop {
            if *cancel.borrow(){return Ok(());}
            wallet.native_invalid.store(true,std::sync::atomic::Ordering::SeqCst);
            health.send_replace(Health::Connecting);
            let connected=tokio::time::timeout(Duration::from_secs(10),tokio_tungstenite::connect_async_with_config(&c.ws_url,Some(tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(65536)).max_frame_size(Some(65536))),false)).await;
            let mut ws=match connected {Ok(Ok((ws,_)))=>ws,_=>{
                reconnects+=1;if reconnects>5{return Err("WS reconnect exhausted".into());}
                tokio::select!{_=cancel.changed()=>return Ok(()),_=tokio::time::sleep(Duration::from_millis(100*reconnects as u64))=>{}}continue;
            }};
            ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["logs",{"address":filter.address,"topics":filter.topics}]}).to_string().into())).await.map_err(|_|"WS subscribe send failed")?;
            let ack=tokio::time::timeout(Duration::from_secs(10),ws.next()).await.map_err(|_|"WS subscribe timeout")?.ok_or("WS closed during subscribe")?.map_err(|_|"WS subscribe transport failed")?;
            let ack:Value=serde_json::from_str(ack.to_text().map_err(|_|"WS subscription ack not JSON text")?).map_err(|_|"WS subscription ack malformed")?;
            if ack["id"]!=1 || ack.get("error").is_some() || ack["jsonrpc"]!="2.0" {return Err("eth_subscribe rejected/malformed".into());}
            let sub=ack["result"].as_str().filter(|s|!s.is_empty()&&s.len()<=256).ok_or("subscription id missing")?.to_owned();
            let chain=rpc(&c,"eth_chainId",json!([])).await?;
            if crate::uint::uint256(chain.as_str().ok_or("chainId missing")?)?!=crate::uint::uint256(&c.chain_id)?{return Err("RPC chainId changed".into());}
            wallet.native_invalid.store(false,std::sync::atomic::Ordering::SeqCst);
            backfill(&c,&mut cursor,&tx,&mut seen).await?;
            health.send_replace(Health::Ready);
            reconnects=0;
            let mut tick=tokio::time::interval(Duration::from_millis(250));
            let mut last=Instant::now();
            loop {
                tokio::select! {
                    biased;
                    _=cancel.changed()=> {let _=ws.close(None).await;return Ok(());},
                    msg=ws.next()=> {
                        let text=match msg {Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t)))=>t,Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(p)))=>{ws.send(tokio_tungstenite::tungstenite::Message::Pong(p)).await.map_err(|_|"WS pong failed")?;continue;},Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_)))=>{last=Instant::now();continue;},Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))|None|Some(Err(_))=>break,_=>return Err("unsupported WS frame".into())};
                        last=Instant::now();
                        let v:Value=serde_json::from_str(&text).map_err(|_|"WS malformed notification")?;
                        if v["jsonrpc"]!="2.0" || v["method"]!="eth_subscription" || v["params"]["subscription"]!=sub{return Err("WS unexpected subscription notification".into());}
                        let e=&v["params"]["result"];
                        if e.get("removed").and_then(Value::as_bool)==Some(true){
                            // A reorg notification is not a trigger: ignore it and keep listening.
                            continue;
                        }
                        event_id(&c,e)?;
                        if c.confirmation_depth==0{enqueue(&c,e.clone(),"ws",&tx,&mut seen)?;}
                    },
                    _=tick.tick()=> {
                        if last.elapsed()>Duration::from_secs(30){break;}
                        if clock()?>=c.deadline_ms{return Err("native task deadline expired".into());}
                        backfill(&c,&mut cursor,&tx,&mut seen).await?;
                        ws.send(tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into())).await.map_err(|_|"WS ping failed")?;
                    }
                }
            }
            reconnects+=1;if reconnects>5{return Err("WS reconnect exhausted".into());}
        }
    }.await;
    if let Err(e) = result {
        wallet
            .native_invalid
            .store(true, std::sync::atomic::Ordering::SeqCst);
        health.send_replace(Health::Fault(e));
    }
}
impl Wallet {
    async fn native_run(
        self: Arc<Self>,
        c: Config,
        binding: executor::Binding,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) {
        let (tx, mut events) = tokio::sync::mpsc::channel(c.queue_capacity);
        let (health_tx, mut health) = tokio::sync::watch::channel(Health::Connecting);
        let (stop_source, source_cancel) = tokio::sync::watch::channel(false);
        let source_task = tokio::spawn(source(
            self.clone(),
            c.clone(),
            tx,
            health_tx,
            source_cancel,
        ));
        let deadline = c
            .monotonic_deadline
            .map(tokio::time::Instant::from_std)
            .unwrap_or_else(tokio::time::Instant::now);
        let outcome=async {
            loop {
                tokio::select! {
                    biased;
                    _=cancel.changed()=>return Ok::<_,String>(("DISARMED",json!({"submission_attempted":false}))),
                    _=tokio::time::sleep_until(deadline)=>return Ok(("FAILED",json!({"error":"deadline expired","submission_attempted":false}))),
                    changed=health.changed()=> {
                        if changed.is_err(){return Err("native source stopped unexpectedly".into());}
                        let h=health.borrow().clone();
                        match h {
                            Health::Fault(e)=>return Err(e),
                            Health::Ready|Health::Connecting=>{
                                let mut b=self.native.lock().await;
                                let r=b.current.as_mut().ok_or("native task missing")?;
                                if r.status=="DISARMED"{return Ok(("DISARMED",json!({"submission_attempted":false})));}
                                r.status=if matches!(h,Health::Ready){"READY"}else{"CONNECTING"}.into();b.persist(&self.entry.dir)?;
                            }
                        }
                    },
                    event=events.recv()=> {
                        let event=event.ok_or("native event source closed")?;
                        if self.native_invalid.load(std::sync::atomic::Ordering::SeqCst){return Err("event received from unsafe source".into());}
                        let id=event_id(&c,&event.value)?;
                        {
                            let mut b=self.native.lock().await;
                            let r=b.current.as_mut().ok_or("native task missing")?;
                            if *cancel.borrow() || self.native_stopping.load(std::sync::atomic::Ordering::SeqCst) || r.status=="DISARMED"{return Ok(("DISARMED",json!({"submission_attempted":false})));}
                            r.operation_id=Some(id.clone());r.status="ACCEPTED".into();r.detail=json!({"event":event.value,"source":event.origin});
                            b.persist(&self.entry.dir)?;
                        }
                        let build_start=Instant::now();
                        let call=match tokio::time::timeout_at(deadline,c.build(&event.value)).await {
                            Ok(Ok(call))=>call,
                            Ok(Err(e))=>return Ok(("FAILED",json!({"error":e,"submission_attempted":false,"stage":"action_build"}))),
                            Err(_)=>return Ok(("FAILED",json!({"error":"action build deadline expired","submission_attempted":false})))
                        };
                        let (call,route_note)=call;
                        let action_build_ms=build_start.elapsed().as_secs_f64()*1000.0;
                        if self.native_invalid.load(std::sync::atomic::Ordering::SeqCst){return Ok(("FAILED",json!({"error":"source became unsafe before fire","submission_attempted":false})));}
                        let before_fire=Instant::now();
                        // Anything provably rejected before submission is a FAILED task, not an
                        // unknown outcome: it must not be reported as "maybe sent, reconcile".
                        if let Err(e)=c.validate_call(&call) {
                            return Ok(("FAILED",json!({"error":e,"submission_attempted":false,"stage":"precedes_fire"})));
                        }
                        let result=self.native_fire(id,(c.deadline_ms,deadline.into_std()),call,c.policy.clone(),binding.clone(),event.received).await;
                        let metrics=json!({"event_to_build_start_ms":build_start.duration_since(event.received).as_secs_f64()*1000.0,"action_build_ms":action_build_ms,"build_to_fire_gap_ms":before_fire.duration_since(build_start).as_secs_f64()*1000.0-action_build_ms,"event_to_result_ms":event.received.elapsed().as_secs_f64()*1000.0,"clock":"monotonic","source":event.origin});
                        // A failure before the execute POST is provably not submitted, so it must be
                        // reported as FAILED rather than sent to journal reconciliation as UNKNOWN.
                        let submitted=self.native_submitted.load(std::sync::atomic::Ordering::SeqCst);
                        return Ok(fire_outcome(submitted,result,&route_note,metrics));
                    }
                }
            }
        }.await;
        stop_source.send_replace(true);
        source_task.abort();
        let _ = source_task.await;
        let (status, detail) =
            outcome.unwrap_or_else(|e| ("FAILED", json!({"error":e,"submission_attempted":false})));
        if let Err(e) = self.native_finish(&c.task_id, status, detail.clone()).await {
            let mut b = self.native.lock().await;
            if let Some(r) = b.current.as_mut().filter(|r| r.config.task_id == c.task_id) {
                // Keep the computed status and the whole computed detail: a persist fault must not
                // relabel a submitted operation as unsent or drop its result/route/metrics. The
                // marker says only that the on-disk copy is stale.
                r.status = status.to_string();
                r.detail = persist_failure_detail(status, detail, &e);
            }
        }
    }
}
/// The terminal record could not be written. Merge the fault into the computed evidence instead of
/// replacing it: a fire that reached the execute POST stays `submission_attempted:true` and keeps
/// its reconcile instruction, and the original operation error is never overwritten by a disk one.
pub(super) fn persist_failure_detail(status: &str, detail: Value, error: &str) -> Value {
    let submitted = matches!(status, "SUCCESS" | "UNKNOWN");
    let mut merged = match detail {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            if !other.is_null() {
                map.insert("detail".into(), other);
            }
            map
        }
    };
    if !merged.contains_key("error") {
        merged.insert("error".into(), json!(error));
    }
    merged.insert("persistence_error".into(), json!(error));
    merged.insert("persistence_failed".into(), json!(true));
    merged.insert("submission_attempted".into(), json!(submitted));
    if submitted {
        // The on-disk copy still shows EXECUTING/nothing terminal, so the journal is authoritative.
        merged.insert("retry".into(), json!("never; reconcile operation journal"));
    } else {
        merged.remove("retry");
    }
    Value::Object(merged)
}
fn rpc_client() -> Result<&'static reqwest::Client, String> {
    // No environment proxies: HTTPS_PROXY would let an outside process answer RPC calls and
    // fabricate the very events that authorize a transaction. One client is reused so the hot
    // path keeps its TLS session instead of paying a handshake per call.
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|_| "RPC client failed".to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}
/// Map a fire result to a terminal status. Only reaching the execute POST may be called unknown;
/// anything failing before it is a plain failure and must not trigger journal reconciliation.
pub(super) fn fire_outcome(
    submitted: bool,
    result: Result<Value, String>,
    route: &Value,
    metrics: Value,
) -> (&'static str, Value) {
    match result {
        Ok(result) => (
            "SUCCESS",
            json!({"success_scope":"backend_acknowledged_not_chain_confirmation","result":result,"route":route,"metrics":metrics}),
        ),
        Err(error) if !submitted => (
            "FAILED",
            json!({"error":error,"submission_attempted":false,"route":route,"metrics":metrics}),
        ),
        Err(error) => (
            "UNKNOWN",
            json!({"error":error,"route":route,"metrics":metrics,"submission_attempted":true,"retry":"never; reconcile operation journal"}),
        ),
    }
}
async fn rpc(c: &Config, method: &str, params: Value) -> Result<Value, String> {
    let client = rpc_client()?;
    let mut response = client
        .post(&c.rpc_url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .map_err(|_| "RPC transport failed")?;
    if !response.status().is_success() {
        return Err("RPC HTTP failure".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "RPC body failed")? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("RPC response limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "malformed RPC JSON")?;
    if v["id"] != 1 || v["jsonrpc"] != "2.0" || v.get("error").is_some() {
        return Err("RPC protocol/error response".into());
    }
    v.get("result").cloned().ok_or("RPC result missing".into())
}
