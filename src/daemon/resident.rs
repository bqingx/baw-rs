//! Bounded owner-only native IPC; one async FIFO lock and resident client per wallet.
use crate::{
    cli::{envelope, Cli, Command},
    core::{client::Client, session},
    daemon::wallets::Entry,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::File,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Mutex, Notify, RwLock, Semaphore},
};
#[cfg(test)]
mod tests;
use crate::daemon::overview;
pub(crate) use crate::task::inspection::describe_definition;
pub(crate) use crate::task::runtime::Control as PipelineControl;
use crate::task::runtime::{self as task_runtime, Scheduler};
const MAX_FRAME: usize = 65536;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(360);
/// Cheap fingerprint of `session.json`, used to decide whether the cached credentials are
/// still the ones on disk. A change of device, inode, size or mtime forces a full reload.
pub(crate) type Stamp = (u64, u64, u64, i64, i64, i64, i64);

pub(crate) fn session_stamp(dir: &std::path::Path) -> Option<Stamp> {
    file_stamp(&dir.join("session.json"))
}
pub(crate) fn file_stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o7777 != 0o600
        || meta.nlink() != 1
    {
        return None;
    }
    Some((
        meta.dev(),
        meta.ino(),
        meta.size(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
pub enum Request {
    Run { name: String, argv: Vec<String> },
    Ping { id: String },
    Pipeline { command: PipelineControl },
    Status,
    Overview { refresh: bool },
    Stop,
}
pub(crate) struct State {
    client: Option<Client>,
    persisted: Option<session::Credentials>,
    /// Identity string the client was built with, reused by the fast path.
    instance: Option<String>,
    /// `(dev, ino, size, mtime_nsec)` of `session.json` at the last full load.
    stamp: Option<Stamp>,
    identity_stamp: Option<Stamp>,
}
pub(crate) struct Wallet {
    pub(crate) entry: Entry,
    pub(crate) state: Mutex<State>,
    _lease: File,
    consumed: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    #[cfg(test)]
    origin: Option<String>,
}
enum WalletRequest<'a> {
    Owned(Box<Cli>),
    Compiled(&'a Cli),
}
impl WalletRequest<'_> {
    fn cli(&self) -> &Cli {
        match self {
            Self::Owned(cli) => cli,
            Self::Compiled(cli) => cli,
        }
    }
    fn bind_instance(&mut self, instance: &str) {
        if let Self::Owned(cli) = self {
            cli.instance_id = instance.to_owned();
        }
    }
}
impl Wallet {
    pub(crate) fn new(entry: Entry) -> Result<Self, String> {
        entry.check()?;
        let lease = crate::util::fsx::private_file(&entry.dir.join(".baw-rs-resident.lock"), true)?;
        fs2::FileExt::try_lock_exclusive(&lease)
            .map_err(|_| "wallet already owned by another resident daemon")?;
        Ok(Self {
            entry,
            state: Mutex::new(State {
                client: None,
                persisted: None,
                instance: None,
                stamp: None,
                identity_stamp: None,
            }),
            _lease: lease,
            consumed: Default::default(),
            #[cfg(test)]
            origin: None,
        })
    }
    pub(crate) fn build(&self, c: session::Credentials) -> Result<Client, String> {
        #[cfg(test)]
        if let Some(origin) = &self.origin {
            return Client::test_client(c, origin);
        }
        Client::new(c)
    }
    /// Shared read path for receipt reconciliation, independent of legacy task books.
    pub(crate) async fn chain_address(&self, chain: &str) -> Result<String, String> {
        self.preload().await?;
        let binding = self.pipeline_binding()?;
        let mut state = self.state.lock().await;
        if self.pipeline_binding()? != binding {
            return Err("wallet changed before address query".into());
        }
        let data = state
            .client
            .as_mut()
            .ok_or("wallet client unavailable")?
            .cached_addresses()
            .await?;
        if self.pipeline_binding()? != binding {
            return Err("wallet changed during address query".into());
        }
        data["addresses"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["binanceChainId"].as_str() == Some(chain))
            })
            .and_then(|row| row["address"].as_str())
            .map(String::from)
            .ok_or_else(|| format!("wallet has no address on chain {chain}"))
    }

    /// Bind task consent to the registered directory, session and identity, without
    /// persisting credentials. Re-authentication requires a new reviewed task.
    pub(crate) fn pipeline_binding(&self) -> Result<String, String> {
        self.entry.check()?;
        let session = session_stamp(&self.entry.dir).ok_or("wallet session missing or unsafe")?;
        let identity =
            file_stamp(&self.entry.instance_file).ok_or("wallet identity missing or unsafe")?;
        Ok(crate::util::validate::sha256_hex(
            &serde_json::to_vec(&(self.entry.device, self.entry.inode, session, identity))
                .map_err(|_| "cannot bind wallet identity")?,
        ))
    }

    /// Local-only session preparation, before a task starts waiting for its trigger.
    pub(crate) async fn preload(&self) -> Result<(), String> {
        self.entry.check()?;
        let mut state = self.state.lock().await;
        let stamp = session_stamp(&self.entry.dir).ok_or("wallet session missing or unsafe")?;
        let identity_stamp =
            file_stamp(&self.entry.instance_file).ok_or("wallet identity missing or unsafe")?;
        if state.client.is_some()
            && state.stamp == Some(stamp)
            && state.identity_stamp == Some(identity_stamp)
        {
            return Ok(());
        }
        let entry = self.entry.clone();
        let (instance, credentials) = tokio::task::spawn_blocking(move || {
            entry.check()?;
            let instance = session::resolve_instance(String::new(), Some(&entry.instance_file))?;
            let credentials = session::load(&entry.dir, &instance)?;
            Ok::<_, String>((instance, credentials))
        })
        .await
        .map_err(|_| "wallet preload worker failed")??;
        if credentials.session_id.is_empty() {
            return Err("wallet session missing".into());
        }
        if session_stamp(&self.entry.dir) != Some(stamp)
            || file_stamp(&self.entry.instance_file) != Some(identity_stamp)
        {
            return Err("wallet identity/session changed during preparation".into());
        }
        let mut client = self.build(session::Credentials {
            client_id: credentials.client_id.clone(),
            session_id: credentials.session_id.clone(),
        })?;
        client.payment_consumed = self.consumed.clone();
        state.client = Some(client);
        state.persisted = Some(credentials);
        state.instance = Some(instance);
        state.stamp = Some(stamp);
        state.identity_stamp = Some(identity_stamp);
        Ok(())
    }

    /// Parse one forwarded argv as if it were a local command, then bind the wallet
    /// directory and the identity placeholder the daemon owns. Neither selector exists on
    /// the command line any more, so a client can never override them.
    pub(crate) fn compile(&self, argv: Vec<String>) -> Result<Cli, String> {
        validate_argv(&argv)?;
        let mut full = vec!["baw-rs".to_owned()];
        full.extend(argv);
        let mut cli = Cli::try_parse_from(full).map_err(|_| "invalid daemon command")?;
        cli.wallet_dir = Some(self.entry.dir.clone());
        cli.instance_id = "daemon-placeholder".into();
        crate::exec::dispatch::preflight(&cli)?;
        Ok(cli)
    }
    async fn execute(&self, argv: Vec<String>) -> Result<Value, String> {
        self.execute_request(WalletRequest::Owned(Box::new(self.compile(argv)?)))
            .await
    }
    pub(crate) async fn execute_compiled(&self, cli: &Cli) -> Result<Value, String> {
        if cli.wallet_dir.as_deref() != Some(self.entry.dir.as_path())
            || cli.skip_prechecks
            || !matches!(
                &cli.command,
                Command::ContractCall {
                    command: crate::exec::signing::ContractCommand::Fire(_)
                } | Command::MarketOrder {
                    command: crate::trade::MarketCommand::Swap(_)
                }
            )
        {
            return Err(
                "compiled task plan is not bound to this wallet or supported action".into(),
            );
        }
        self.execute_request(WalletRequest::Compiled(cli)).await
    }
    async fn execute_request(&self, mut request: WalletRequest<'_>) -> Result<Value, String> {
        crate::task::timing::wallet_queue_entered();
        let mut state = tokio::time::timeout(Duration::from_secs(30), self.state.lock())
            .await
            .map_err(|_| "selected wallet busy; request not executed")?;
        crate::task::timing::wallet_lock_acquired();
        let entry = self.entry.clone();
        let auth_command = match &request.cli().command {
            Command::Auth { command } => Some(command.clone()),
            _ => None,
        };
        let auth = auth_command.is_some();
        // Revalidate the wallet directory itself (device, inode, owner) on every request;
        // this is a stat, not a decrypt.
        entry.check()?;
        let stamp = session_stamp(&entry.dir);
        // Fast path: the session file is byte-for-byte the one we last decrypted (same
        // device, inode, size and mtime), so the credentials already in memory are reused
        // instead of paying PBKDF2 (100k rounds) plus AES on every command. Any change to
        // the file — including a swap with a same-sized replacement — changes the inode or
        // mtime and forces the full load below. Auth commands always take the slow path.
        let identity_stamp = file_stamp(&entry.instance_file);
        if !auth
            && state.client.is_some()
            && stamp.is_some()
            && stamp == state.stamp
            && identity_stamp == state.identity_stamp
        {
            let instance = state
                .instance
                .as_deref()
                .ok_or("wallet instance unavailable in resident state")?;
            request.bind_instance(instance);
            let mut client = state.client.take().ok_or("wallet client unavailable")?;
            crate::task::timing::dispatch_started();
            let result = tokio::time::timeout(
                COMMAND_TIMEOUT,
                crate::exec::dispatch::execute(&mut client, request.cli()),
            )
            .await;
            // Same rule as the slow path: an uncertain outcome drops the client instead of
            // retaining a possibly rotated cookie, and forces the next request to reload.
            if result.is_ok() {
                state.client = Some(client);
            }
            return result.map_err(|_| {
                "command timed out; mutation outcome uncertain; inspect operation journal before retry"
                    .to_owned()
            })?;
        }
        // Session preloading/PBKDF2 runs off-runtime, never under a global wallet lock.
        let loaded = tokio::task::spawn_blocking(move || {
            entry.check()?;
            let instance = session::resolve_instance(String::new(), Some(&entry.instance_file))?;
            let credentials = if let Some(command) = auth_command {
                crate::core::auth::bootstrap(&entry.dir, &instance, &command)?
            } else {
                session::load(&entry.dir, &instance)?
            };
            Ok::<_, String>((instance, credentials))
        })
        .await
        .map_err(|_| "wallet load worker failed")?;
        let (instance, credentials) = match loaded {
            Ok(v) => v,
            Err(e) => {
                state.client = None;
                state.persisted = None;
                return Err(e);
            }
        };
        request.bind_instance(&instance);
        let changed = state.persisted.as_ref().is_none_or(|old| {
            old.client_id != credentials.client_id || old.session_id != credentials.session_id
        });
        if changed || auth || state.client.is_none() {
            let mut client = self.build(session::Credentials {
                client_id: credentials.client_id.clone(),
                session_id: credentials.session_id.clone(),
            })?;
            client.payment_consumed = self.consumed.clone();
            state.client = Some(client);
        }
        state.instance = Some(instance);
        state.stamp = stamp;
        state.identity_stamp = identity_stamp;
        state.persisted = Some(credentials);
        // Take ownership: cancellation must drop, not retain, an uncertain in-memory cookie.
        let mut client = state.client.take().ok_or("wallet client unavailable")?;
        crate::task::timing::dispatch_started();
        let result = tokio::time::timeout(
            COMMAND_TIMEOUT,
            crate::exec::dispatch::execute(&mut client, request.cli()),
        )
        .await;
        if auth {
            state.persisted = None;
        } else if result.is_ok() {
            state.client = Some(client);
        }
        result.map_err(|_|"command timed out; mutation outcome uncertain; inspect operation journal before retry".to_owned())?
    }
}
/// Defense in depth for daemon ingress. `--wallet` and `--registry` are selectors the caller
/// already used, and `--wallet-dir`/`--instance-id*` are retired flags: clap would reject them
/// as unknown, but a forwarded frame must never look like it could rebind the wallet either.
pub(crate) fn validate_argv(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() || argv.len() > 128 {
        return Err("argv must contain 1..128 strings".into());
    }
    if argv.iter().any(|a| {
        matches!(
            a.split('=').next().unwrap_or(""),
            "--wallet-dir"
                | "--instance-id"
                | "--instance-id-file"
                | "--wallet"
                | "--registry"
                | "--wallet-name"
        )
    }) {
        return Err("wallet selector overrides are forbidden in daemon requests".into());
    }
    Ok(())
}
async fn read_frame(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let capacity = chunk.len().min(max.saturating_sub(bytes.len()) + 1);
        let n = stream
            .read(&mut chunk[..capacity])
            .await
            .map_err(|_| "IPC read failed")?;
        if n == 0 {
            return Err("incomplete IPC frame".into());
        }
        if let Some(end) = chunk[..n].iter().position(|b| *b == b'\n') {
            bytes.extend_from_slice(&chunk[..end]);
            return Ok(bytes);
        }
        if bytes.len() + n > max {
            return Err("IPC frame too large".into());
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}
fn peer(stream: &UnixStream) -> Result<(), String> {
    if stream
        .peer_cred()
        .map_err(|_| "cannot inspect IPC peer")?
        .uid()
        != unsafe { libc::geteuid() }
    {
        return Err("IPC peer uid mismatch".into());
    }
    Ok(())
}
struct SocketGuard(PathBuf);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(crate) async fn serve(
    path: &Path,
    entries: BTreeMap<String, Entry>,
    registry: PathBuf,
) -> Result<Value, String> {
    serve_inner(path, entries, registry).await
}
async fn serve_inner(
    path: &Path,
    entries: BTreeMap<String, Entry>,
    registry: PathBuf,
) -> Result<Value, String> {
    crate::util::fsx::private_dir(path.parent().ok_or("invalid socket path")?)?;
    // Serialize bind/stale cleanup, including empty registries.
    let lease = crate::util::fsx::private_file(&path.with_extension("lock"), true)?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(|_| "daemon already running")?;
    let mut wallets = BTreeMap::new();
    for (name, entry) in entries {
        wallets.insert(name, Arc::new(Wallet::new(entry)?));
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err("refusing to replace non-socket IPC path".into());
        }
        match tokio::time::timeout(FRAME_TIMEOUT, UnixStream::connect(path)).await {
            Ok(Ok(_)) => return Err("daemon already running".into()),
            Err(_) => return Err("existing daemon socket timed out; refusing replacement".into()),
            Ok(Err(_)) => {}
        }
        std::fs::remove_file(path).map_err(|_| "cannot remove stale socket")?;
    }
    // Resident processes reuse the pre-write checks for a few seconds: a preview and the
    // execute that follows it then share one settings read and one address read instead of
    // paying four round trips. One-shot commands keep the strict default (no reuse).
    crate::exec::signing::set_skip_prechecks_default(
        std::env::var("BAW_RS_SKIP_PRECHECKS").is_ok_and(|v| v == "1" || v == "true"),
    );
    crate::core::client::set_default_check_ttl_ms(3000);
    let listener = UnixListener::bind(path).map_err(|_| "cannot bind daemon socket")?;
    let _cleanup = SocketGuard(path.to_owned());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot restrict IPC socket")?;
    serve_listener(listener, Arc::new(RwLock::new(wallets)), registry).await
}

/// Resolve a wallet for one request, re-reading the registry when the name is not loaded yet.
///
/// A wallet added with `wallets add` while the daemon runs is picked up on first use instead of
/// requiring `wallets stop` → `add` → `serve`, which would interrupt every other wallet. Only
/// additions are hot: a removed wallet keeps running until restart, because dropping it could
/// orphan in-flight events or tasks. Validation is the same `Entry::check` used at startup.
pub(crate) async fn wallet_for(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    name: &str,
    registry: &Path,
) -> Result<Arc<Wallet>, String> {
    if let Some(wallet) = wallets.read().await.get(name) {
        return Ok(wallet.clone());
    }
    let entries = crate::daemon::wallets::reload_entries(registry)?;
    let mut loaded = wallets.write().await;
    if let Some(entry) = entries.get(name) {
        if let std::collections::btree_map::Entry::Vacant(slot) = loaded.entry(name.to_owned()) {
            slot.insert(Arc::new(Wallet::new(entry.clone())?));
        }
    }
    loaded
        .get(name)
        .cloned()
        .ok_or_else(|| "unknown wallet name; no fallback".to_owned())
}
/// Everything a connection needs, so the handler signature stays readable.
pub(crate) struct Daemon {
    pub(crate) wallets: Arc<RwLock<BTreeMap<String, Arc<Wallet>>>>,
    pub(crate) registry: PathBuf,
    pub(crate) scheduler: Arc<Scheduler>,
}

async fn serve_listener(
    listener: UnixListener,
    wallets: Arc<RwLock<BTreeMap<String, Arc<Wallet>>>>,
    registry: PathBuf,
) -> Result<Value, String> {
    let daemon = Arc::new(Daemon {
        // Cloned, not moved: shutdown still walks the wallet set directly.
        wallets: wallets.clone(),
        registry,
        scheduler: Arc::new(task_runtime::Scheduler::default()),
    });
    let stop = Arc::new(Notify::new());
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let permits = Arc::new(Semaphore::new(64));
    let persistent_permits = Arc::new(Semaphore::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=stop.notified()=>break,
            Some(_)=tasks.join_next(), if !tasks.is_empty()=>{},
            accepted=listener.accept()=>{
                let (stream,_)=accepted.map_err(|_|"IPC accept failed")?;
                if *shutdown.borrow() || peer(&stream).is_err(){continue;}
                let Ok(permit)=permits.clone().try_acquire_owned() else{continue;};
                let daemon=daemon.clone(); let stop=stop.clone(); let persistent_permits=persistent_permits.clone();
                tasks.spawn(handle(stream,daemon,stop,permit,persistent_permits,shutdown.clone()));
            }
        }
    }
    // Stop pending pipeline work and drain admitted wallet requests before exit.
    drop(listener);
    shutdown.send_replace(true);
    daemon.scheduler.stop().await;
    while tasks.join_next().await.is_some() {}
    Ok(json!({"status":"stopped"}))
}
async fn handle(
    stream: UnixStream,
    daemon: Arc<Daemon>,
    stop: Arc<Notify>,
    permit: tokio::sync::OwnedSemaphorePermit,
    persistent_permits: Arc<Semaphore>,
    shutdown: tokio::sync::watch::Sender<bool>,
) {
    // Cheap clones: the body refers to these by name throughout.
    let wallets = daemon.wallets.clone();
    let registry = daemon.registry.clone();
    let mut shutdown_rx = shutdown.subscribe();
    let mut legacy_permit = Some(permit);
    let mut persistent_permit = None;
    let mut stream = tokio::io::BufReader::new(stream);
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        let frame = tokio::select! {
            biased;
            _=shutdown_rx.changed()=>break,
            frame=tokio::time::timeout(FRAME_TIMEOUT, read_buffered_frame(&mut stream, MAX_FRAME))=>frame,
        };
        // Only work admitted before shutdown may drain; never dispatch subsequent frames.
        if *shutdown_rx.borrow() {
            break;
        }
        let received = std::time::Instant::now();
        let request = match frame {
            Ok(Ok(bytes)) => serde_json::from_slice::<Request>(&bytes)
                .map_err(|_| "invalid IPC request".to_owned()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("IPC read timeout".into()),
        };
        let id = match &request {
            Ok(Request::Ping { id }) => Some(id.clone()),
            _ => None,
        };
        let persistent = id.is_some();
        let ingress_decode_ms = received.elapsed().as_secs_f64() * 1000.0;
        if persistent && persistent_permit.is_none() {
            match persistent_permits.clone().try_acquire_owned() {
                Ok(permit) => {
                    persistent_permit = Some(permit);
                    legacy_permit.take();
                }
                Err(_) => {
                    let response = json!({"success":false,"id":id,"error":{"message":"BUSY; persistent connection limit"}});
                    let _ = tokio::time::timeout(
                        FRAME_TIMEOUT,
                        stream
                            .get_mut()
                            .write_all(format!("{response}\n").as_bytes()),
                    )
                    .await;
                    break;
                }
            }
        }
        let mut stopping = false;
        let result = match request {
            Ok(Request::Pipeline { command }) => {
                daemon.scheduler.control(daemon.clone(), command).await
            }
            Ok(Request::Ping { .. }) => Ok(json!({"status":"ready"})),
            Err(e) => Err(e),
            Ok(Request::Status) => Ok({
                let names = wallets.read().await.keys().cloned().collect::<Vec<_>>();
                json!({"status":"running","wallets":names,"registry":registry,"hotReload":true,"pid":std::process::id(),"notifications":daemon.scheduler.notification_status()})
            }),
            Ok(Request::Overview { refresh }) => overview::run(&daemon, refresh).await,
            Ok(Request::Stop) => {
                shutdown.send_replace(true);
                stop.notify_one();
                stopping = true;
                Ok(json!({"status":"stopping"}))
            }
            Ok(Request::Run { name, argv }) => match wallet_for(&wallets, &name, &registry).await {
                Ok(wallet) => {
                    crate::exec::execution::rpc_scope(
                        daemon.scheduler.rpc.clone(),
                        wallet.execute(argv),
                    )
                    .await
                }
                Err(error) => Err(error),
            },
        };
        let mut response = envelope(result);
        if persistent {
            response["metrics"] = json!({"ingress_decode_ms":ingress_decode_ms,"queue_ms":0.0,"queue_policy":"reject_busy","queue_scope":"admitted_application_work_only; socket backlog excluded","handler_ms":received.elapsed().as_secs_f64()*1000.0});
        }
        if let Some(id) = id {
            response["id"] = json!(id);
        }
        let mut bytes = serde_json::to_vec(&response).unwrap_or_default();
        if bytes.len() > MAX_RESPONSE {
            bytes = serde_json::to_vec(&envelope(Err("IPC response too large".into())))
                .unwrap_or_default();
        }
        bytes.push(b'\n');
        let written = tokio::time::timeout(FRAME_TIMEOUT, stream.get_mut().write_all(&bytes)).await;
        if stopping {
            stop.notify_one();
        }
        if !persistent || *shutdown_rx.borrow() || !matches!(written, Ok(Ok(()))) {
            break;
        }
    }
}
async fn read_buffered_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncBufReadExt;
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|_| "IPC read failed")?;
        if available.is_empty() {
            return Err("incomplete IPC frame".into());
        }
        let end = available.iter().position(|b| *b == b'\n');
        let count = end.map_or(available.len(), |n| n + 1);
        if bytes.len() + count > max + 1 {
            return Err("IPC frame too large".into());
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            bytes.pop();
            return Ok(bytes);
        }
    }
}
pub async fn send(path: &Path, request: Request) -> Result<Value, String> {
    crate::util::fsx::private_dir(path.parent().ok_or("invalid socket path")?)?;
    let mut stream = tokio::time::timeout(FRAME_TIMEOUT, UnixStream::connect(path))
        .await
        .map_err(|_| "daemon connection timeout")?
        .map_err(|_| "daemon unavailable; no fallback")?;
    peer(&stream)?;
    let mut bytes = serde_json::to_vec(&request).map_err(|_| "cannot encode IPC request")?;
    if bytes.len() > MAX_FRAME {
        return Err("IPC request too large".into());
    }
    bytes.push(b'\n');
    tokio::time::timeout(FRAME_TIMEOUT, stream.write_all(&bytes))
        .await
        .map_err(|_| "IPC write timeout")?
        .map_err(|_| "IPC write failed")?;
    let bytes = tokio::time::timeout(
        Duration::from_secs(400),
        read_frame(&mut stream, MAX_RESPONSE),
    )
    .await
    .map_err(|_| "IPC response timeout; outcome uncertain, never blindly retry")??;
    let response: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid IPC response")?;
    if response["success"] == true {
        Ok(response["data"].clone())
    } else {
        Err(response["error"]["message"]
            .as_str()
            .unwrap_or("daemon command failed")
            .to_owned())
    }
}
