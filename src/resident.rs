//! Bounded owner-only native IPC; one async FIFO lock and resident client per wallet.
use crate::{
    cli::{envelope, Cli, Command},
    client::Client,
    session,
    vaults::Entry,
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
    sync::{Mutex, Notify, Semaphore},
};
#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
const MAX_FRAME: usize = 65536;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(360);
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
pub enum Request {
    Run { name: String, argv: Vec<String> },
    Status,
    Stop,
}
struct State {
    client: Option<Client>,
    persisted: Option<session::Credentials>,
}
pub(crate) struct Wallet {
    entry: Entry,
    state: Mutex<State>,
    _lease: File,
    consumed: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    inline_identity: Option<zeroize::Zeroizing<String>>,
    #[cfg(test)]
    origin: Option<String>,
}
impl Wallet {
    pub(crate) fn new(entry: Entry) -> Result<Self, String> {
        entry.check()?;
        let lease = crate::vaults::private_file(&entry.dir.join(".baw-rs-resident.lock"), true)?;
        fs2::FileExt::try_lock_exclusive(&lease)
            .map_err(|_| "wallet already owned by another resident daemon")?;
        Ok(Self {
            entry,
            state: Mutex::new(State {
                client: None,
                persisted: None,
            }),
            _lease: lease,
            consumed: Default::default(),
            inline_identity: None,
            #[cfg(test)]
            origin: None,
        })
    }
    fn build(&self, c: session::Credentials) -> Result<Client, String> {
        #[cfg(test)]
        if let Some(origin) = &self.origin {
            return Client::test_client(c, origin);
        }
        Client::new(c)
    }
    async fn execute(&self, argv: Vec<String>) -> Result<Value, String> {
        validate_argv(&argv)?;
        let mut full = vec![
            "baw-rs".to_owned(),
            "--wallet-dir".into(),
            self.entry.dir.to_string_lossy().into_owned(),
            "--instance-id".into(),
            "daemon-placeholder".into(),
        ];
        full.extend(argv);
        let mut cli = Cli::try_parse_from(full).map_err(|_| "invalid daemon command")?;
        if matches!(
            cli.command,
            Command::Serve
                | Command::Batch
                | Command::Tracker {
                    command: crate::tracker::TrackerCommand::Ws(_)
                }
        ) {
            return Err("streaming and control commands are not allowed over daemon IPC".into());
        }
        crate::signing::apply_skip_prechecks(cli.skip_prechecks);
        crate::dispatch::preflight(&cli)?;
        let mut state = tokio::time::timeout(Duration::from_secs(30), self.state.lock())
            .await
            .map_err(|_| "selected wallet busy; request not executed")?;
        let entry = self.entry.clone();
        let inline = self.inline_identity.clone();
        let auth_command = match &cli.command {
            Command::Auth { command } => Some(command.clone()),
            _ => None,
        };
        let auth = auth_command.is_some();
        // Session preloading/PBKDF2 runs off-runtime, never under a global wallet lock.
        let loaded = tokio::task::spawn_blocking(move || {
            entry.check()?;
            let instance = match inline {
                Some(s) => s.to_string(),
                None => session::resolve_instance(String::new(), Some(&entry.instance_file))?,
            };
            let credentials = if let Some(command) = auth_command {
                crate::auth::bootstrap(&entry.dir, &instance, &command)?
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
        cli.instance_id = instance;
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
        state.persisted = Some(credentials);
        // Take ownership: cancellation must drop, not retain, an uncertain in-memory cookie.
        let mut client = state.client.take().ok_or("wallet client unavailable")?;
        let result =
            tokio::time::timeout(COMMAND_TIMEOUT, crate::dispatch::execute(&mut client, &cli))
                .await;
        if auth {
            state.persisted = None;
        } else if result.is_ok() {
            state.client = Some(client);
        }
        result.map_err(|_|"command timed out; mutation outcome uncertain; inspect operation journal before retry".to_owned())?
    }
}
fn validate_argv(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() || argv.len() > 128 {
        return Err("argv must contain 1..128 strings".into());
    }
    if argv.iter().any(|a| {
        matches!(
            a.split('=').next().unwrap_or(""),
            "--wallet-dir"
                | "--instance-id"
                | "--instance-id-file"
                | "--warm"
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
pub(crate) async fn serve(path: &Path, entries: BTreeMap<String, Entry>) -> Result<Value, String> {
    serve_inner(path, entries, None).await
}
pub(crate) async fn serve_legacy(cli: &Cli) -> Result<Value, String> {
    use std::os::unix::fs::MetadataExt;
    let dir = cli
        .wallet_dir
        .canonicalize()
        .map_err(|_| "wallet directory must exist")?;
    crate::vaults::private_dir(&dir)?;
    let meta = std::fs::metadata(&dir).map_err(|_| "cannot inspect wallet")?;
    let entry = Entry {
        instance_file: dir.join(".unused-instance"),
        dir: dir.clone(),
        device: meta.dev(),
        inode: meta.ino(),
    };
    serve_inner(
        &crate::warm::socket_path(&dir),
        BTreeMap::from([("single".into(), entry)]),
        Some(cli.instance_id.clone()),
    )
    .await
}
async fn serve_inner(
    path: &Path,
    entries: BTreeMap<String, Entry>,
    inline: Option<String>,
) -> Result<Value, String> {
    crate::vaults::private_dir(path.parent().ok_or("invalid socket path")?)?;
    // Serialize bind/stale cleanup, including empty registries.
    let lease = crate::vaults::private_file(&path.with_extension("lock"), true)?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(|_| "daemon already running")?;
    let mut wallets = BTreeMap::new();
    for (name, entry) in entries {
        let mut wallet = Wallet::new(entry)?;
        wallet.inline_identity = inline.as_ref().map(|s| zeroize::Zeroizing::new(s.clone()));
        wallets.insert(name, Arc::new(wallet));
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
    crate::signing::set_skip_prechecks_default(
        std::env::var("BAW_RS_SKIP_PRECHECKS").is_ok_and(|v| v == "1" || v == "true"),
    );
    if std::env::var_os("BAW_RS_CHECK_TTL_MS").is_none() {
        std::env::set_var("BAW_RS_CHECK_TTL_MS", "3000");
    }
    let listener = UnixListener::bind(path).map_err(|_| "cannot bind daemon socket")?;
    let _cleanup = SocketGuard(path.to_owned());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot restrict IPC socket")?;
    serve_listener(listener, Arc::new(wallets)).await
}
async fn serve_listener(
    listener: UnixListener,
    wallets: Arc<BTreeMap<String, Arc<Wallet>>>,
) -> Result<Value, String> {
    let stop = Arc::new(Notify::new());
    let permits = Arc::new(Semaphore::new(64));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=stop.notified()=>break,
            Some(_)=tasks.join_next(), if !tasks.is_empty()=>{},
            accepted=listener.accept()=>{
                let (stream,_)=accepted.map_err(|_|"IPC accept failed")?;
                if peer(&stream).is_err(){continue;}
                let Ok(permit)=permits.clone().try_acquire_owned() else{continue;};
                let wallets=wallets.clone(); let stop=stop.clone();
                tasks.spawn(async move{let _permit=permit;handle(stream,wallets,stop).await;});
            }
        }
    }
    // Do not cancel submitted mutations on a graceful stop. Idle reads are bounded.
    drop(listener);
    while tasks.join_next().await.is_some() {}
    Ok(json!({"status":"stopped"}))
}
async fn handle(
    mut stream: UnixStream,
    wallets: Arc<BTreeMap<String, Arc<Wallet>>>,
    stop: Arc<Notify>,
) {
    let request =
        match tokio::time::timeout(FRAME_TIMEOUT, read_frame(&mut stream, MAX_FRAME)).await {
            Ok(Ok(bytes)) => serde_json::from_slice::<Request>(&bytes)
                .map_err(|_| "invalid IPC request".to_owned()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("IPC read timeout".into()),
        };
    let mut stopping = false;
    let result = match request {
        Err(e) => Err(e),
        Ok(Request::Status) => Ok(
            json!({"status":"running","wallets":wallets.keys().collect::<Vec<_>>(),"pid":std::process::id()}),
        ),
        Ok(Request::Stop) => {
            stopping = true;
            Ok(json!({"status":"stopping"}))
        }
        Ok(Request::Run { name, argv }) => match wallets.get(&name) {
            None => Err("unknown wallet name; no fallback".into()),
            Some(wallet) => wallet.execute(argv).await,
        },
    };
    let mut bytes = serde_json::to_vec(&envelope(result)).unwrap_or_default();
    if bytes.len() > MAX_RESPONSE {
        bytes =
            serde_json::to_vec(&envelope(Err("IPC response too large".into()))).unwrap_or_default();
    }
    bytes.push(b'\n');
    let _ = tokio::time::timeout(FRAME_TIMEOUT, stream.write_all(&bytes)).await;
    if stopping {
        stop.notify_one();
    }
}
pub async fn send(path: &Path, request: Request) -> Result<Value, String> {
    crate::vaults::private_dir(path.parent().ok_or("invalid socket path")?)?;
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
