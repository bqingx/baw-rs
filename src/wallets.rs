//! Named-wallet control plane. The registry holds paths and filesystem identities, never keys.
//!
//! Every wallet-scoped command names its wallet with the global `--wallet <NAME>` flag; the
//! name is resolved here and forwarded to the resident daemon as `Request::Run { name, argv }`.
//! Management (`wallets add|remove|list|status|serve|stop`) never needs a wallet.
use crate::resident::native_events::Control;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// Environment variable that overrides the default registry location.
pub const REGISTRY_ENV: &str = "BAW_RS_REGISTRY";

#[derive(Args, Debug)]
#[command(about = "Wallet lifecycle and the named resident daemon; never requires --wallet")]
pub struct Wallets {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Action {
    /// Create private wallet paths (0700) and an identity file (0600), then register the name.
    Add {
        name: String,
        /// Directory holding per-wallet subdirectories. Defaults to $HOME/wallets.
        #[arg(long, value_name = "PATH")]
        wallet_root: Option<PathBuf>,
        /// Use this wallet directory instead of <root>/<name>/wallet.
        #[arg(long, value_name = "PATH")]
        wallet_dir: Option<PathBuf>,
        /// Use this identity file path instead of <root>/<name>/identity.key.
        #[arg(long = "instance-id-file", value_name = "PATH")]
        instance_id_file: Option<PathBuf>,
    },
    /// Unregister a name. Wallet data is kept unless --delete-data is given with --confirm.
    Remove {
        name: String,
        /// Explicitly keep the wallet directory and identity file (the default).
        #[arg(long = "keep-data")]
        keep_data: bool,
        /// Delete session, identity and the wallet directory after unregistering (needs --confirm).
        #[arg(long = "delete-data")]
        delete_data: bool,
        #[arg(long)]
        confirm: bool,
    },
    /// Registered names and their paths. Never reads sessions or contacts the backend.
    List,
    /// Resident daemon status: pid and the names it was started with.
    Status,
    /// Run the resident daemon in the foreground for every registered wallet.
    Serve,
    /// Stop accepting work, drain admitted requests, then exit.
    Stop,
    /// Compatibility: register an existing wallet directory and identity file.
    #[command(hide = true)]
    Register {
        name: String,
        #[arg(long)]
        wallet_dir: PathBuf,
        #[arg(long)]
        instance_id_file: PathBuf,
    },
    /// Compatibility: forward argv to the resident daemon under an explicit name.
    #[command(hide = true)]
    Run {
        name: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Compatibility: native event task control with an explicit wallet name.
    #[command(hide = true)]
    Task {
        name: String,
        #[command(subcommand)]
        command: TaskAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum TaskAction {
    Load {
        #[arg(long)]
        config: PathBuf,
    },
    Arm {
        task_id: String,
        #[arg(long)]
        confirm: bool,
    },
    Status,
    Disarm {
        task_id: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub dir: PathBuf,
    pub instance_file: PathBuf,
    pub device: u64,
    pub inode: u64,
}

/// A resolved, registered wallet. Safe to hand to the binary crate.
pub struct Resolved {
    pub name: String,
    pub dir: PathBuf,
    pub instance_file: PathBuf,
}

type Registry = BTreeMap<String, Entry>;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn invalid_name(name: &str) -> String {
    format!("invalid wallet name '{name}': 1..64 ASCII letters, digits, hyphens or underscores")
}

pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "cannot inspect private directory")?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
        return Err("directory must be owned, private (0700), and not a symlink".into());
    }
    Ok(())
}

fn registry_dir(path: &Path, create: bool) -> Result<(), String> {
    if create && !path.exists() {
        create_private_dir(path)?;
    }
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "registry {} does not exist; create it with `baw-rs wallets add NAME --registry {}` or pass --registry/{REGISTRY_ENV}",
            path.display(),
            path.display()
        )),
        Err(_) => Err("cannot inspect registry directory".into()),
        Ok(_) => private_dir(path).map_err(|_| {
            format!(
                "registry {} must be an owned, private (0700) directory",
                path.display()
            )
        }),
    }
}

/// Create `path` (and any missing ancestors) with 0700, then verify the leaf is private.
fn create_private_dir(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok() {
        return private_dir(path);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if std::fs::symlink_metadata(parent).is_err() {
            create_private_dir(parent)?;
        }
    }
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| format!("cannot create directory {}", path.display()))?;
    private_dir(path)
}

pub(crate) fn private_file(path: &Path, create: bool) -> Result<File, String> {
    let f = OpenOptions::new()
        .read(true)
        .write(create)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "cannot open private file")?;
    let m = f.metadata().map_err(|_| "cannot inspect private file")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
    {
        return Err("file must be owned, private (0600), single-link and regular".into());
    }
    Ok(f)
}

fn lock_registry(path: &Path, exclusive: bool) -> Result<File, String> {
    let lock = private_file(&path.join("registry.lock"), true)?;
    let result = if exclusive {
        fs2::FileExt::try_lock_exclusive(&lock)
    } else {
        fs2::FileExt::try_lock_shared(&lock)
    };
    result.map_err(|_| "registry busy; stop daemon before changing registrations")?;
    Ok(lock)
}

fn load(path: &Path) -> Result<Registry, String> {
    match std::fs::symlink_metadata(path.join("registry.json")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err("cannot inspect registry".into()),
        Ok(_) => {}
    }
    let mut bytes = Vec::new();
    private_file(&path.join("registry.json"), false)?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read registry")?;
    if bytes.len() > 1_048_576 {
        return Err("registry too large".into());
    }
    let entries: Registry = serde_json::from_slice(&bytes).map_err(|_| "invalid registry")?;
    if entries.len() > 128 || entries.keys().any(|n| !valid_name(n)) {
        return Err("invalid registry names or too many wallets".into());
    }
    let mut ids = std::collections::HashSet::new();
    for e in entries.values() {
        e.check()?;
        if !ids.insert((e.device, e.inode)) {
            return Err("duplicate wallet directory".into());
        }
    }
    Ok(entries)
}

fn save(path: &Path, entries: &Registry) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(entries).map_err(|_| "cannot encode registry")?;
    let temp = path.join(format!(
        ".registry-{}-{:x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "cannot create registry update")?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "cannot write registry")?;
    std::fs::rename(&temp, path.join("registry.json")).map_err(|_| "cannot replace registry")?;
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| "cannot sync registry")?;
    Ok(())
}

impl Entry {
    pub(crate) fn check(&self) -> Result<(), String> {
        private_dir(&self.dir)?;
        let m = std::fs::metadata(&self.dir).map_err(|_| "cannot inspect registered wallet")?;
        if !self.dir.is_absolute()
            || !self.instance_file.is_absolute()
            || m.dev() != self.device
            || m.ino() != self.inode
        {
            return Err("registered wallet directory identity changed".into());
        }
        Ok(())
    }
}

/// Registry location: an explicit `--registry` wins, then `BAW_RS_REGISTRY`, then
/// `$HOME/wallets/registry`. There is no other fallback.
pub fn registry_path(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        if path.as_os_str().is_empty() {
            return Err("--registry must be a nonempty path".into());
        }
        return Ok(path.to_path_buf());
    }
    if let Ok(value) = std::env::var(REGISTRY_ENV) {
        if !value.trim().is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => Ok(PathBuf::from(home).join("wallets").join("registry")),
        _ => Err(format!(
            "cannot determine registry path: pass --registry or set {REGISTRY_ENV}"
        )),
    }
}

pub fn socket_path(registry: &Path) -> PathBuf {
    registry.join("daemon.sock")
}

/// Resolve a name against the registry. Unknown names and missing registries fail here, so a
/// named command never falls back to another wallet or to the legacy single-wallet paths.
pub fn resolve(registry: &Path, name: &str) -> Result<Resolved, String> {
    if !valid_name(name) {
        return Err(invalid_name(name));
    }
    if std::fs::symlink_metadata(registry).is_err() {
        return Err(format!(
            "unknown wallet '{name}': registry {} does not exist; create it with `baw-rs wallets add {name} --registry {}`",
            registry.display(),
            registry.display()
        ));
    }
    registry_dir(registry, false)?;
    let _lock = lock_registry(registry, false)?;
    let entries = load(registry)?;
    let entry = entries.get(name).ok_or_else(|| {
        format!(
            "unknown wallet '{name}'; not registered in {} (run `baw-rs wallets list --registry {}`)",
            registry.display(),
            registry.display()
        )
    })?;
    entry.check()?;
    Ok(Resolved {
        name: name.to_owned(),
        dir: entry.dir.clone(),
        instance_file: entry.instance_file.clone(),
    })
}

fn register(path: &Path, name: &str, dir: &Path, key: &Path) -> Result<Value, String> {
    if !valid_name(name) {
        return Err(invalid_name(name));
    }
    registry_dir(path, true)?;
    let _lock = lock_registry(path, true)?;
    let mut entries = load(path)?;
    if entries.contains_key(name) || entries.len() >= 128 {
        return Err(format!(
            "wallet name '{name}' already registered or registry full (128 max)"
        ));
    }
    let dir = dir
        .canonicalize()
        .map_err(|_| format!("wallet directory {} must exist", dir.display()))?;
    private_dir(&dir)?;
    // Validate the original path before canonicalizing: instance-file symlinks are forbidden.
    let _ = crate::session::resolve_instance(String::new(), Some(key))?;
    let key = key
        .canonicalize()
        .map_err(|_| format!("instance-id file {} must exist", key.display()))?;
    let m = std::fs::metadata(&dir).map_err(|_| "cannot inspect wallet")?;
    if entries
        .values()
        .any(|e| e.device == m.dev() && e.inode == m.ino())
    {
        return Err("duplicate wallet directory".into());
    }
    entries.insert(
        name.to_owned(),
        Entry {
            dir,
            instance_file: key,
            device: m.dev(),
            inode: m.ino(),
        },
    );
    save(path, &entries)?;
    Ok(json!({"registered":name}))
}

fn random_identity() -> String {
    let bytes: [u8; 32] = rand::random();
    let mut out = String::with_capacity(65);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push('\n');
    out
}

fn add(
    registry: &Path,
    name: &str,
    wallet_root: Option<&Path>,
    wallet_dir: Option<&Path>,
    instance_file: Option<&Path>,
) -> Result<Value, String> {
    if !valid_name(name) {
        return Err(invalid_name(name));
    }
    let root = match wallet_root {
        Some(path) => path.to_path_buf(),
        None => match std::env::var_os("HOME") {
            Some(home) if !home.is_empty() => PathBuf::from(home).join("wallets"),
            _ => return Err("cannot determine wallet root: pass --wallet-root".into()),
        },
    };
    let dir = wallet_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(name).join("wallet"));
    let key = instance_file
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(name).join("identity.key"));
    if std::fs::symlink_metadata(&dir).is_ok() {
        return Err(format!(
            "wallet directory {} already exists; use `baw-rs wallets register {name} --wallet-dir ... --instance-id-file ...` to register an existing wallet",
            dir.display()
        ));
    }
    if std::fs::symlink_metadata(&key).is_ok() {
        return Err(format!(
            "identity file {} already exists; refusing to overwrite it",
            key.display()
        ));
    }
    create_private_dir(&root)?;
    create_private_dir(&dir)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&key)
        .map_err(|_| "cannot create identity file")?;
    file.write_all(random_identity().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot write identity file")?;
    drop(file);
    private_file(&key, false)?;
    register(registry, name, &dir, &key)?;
    Ok(json!({
        "added": name,
        "walletDir": dir.canonicalize().unwrap_or(dir),
        "instanceIdFile": key.canonicalize().unwrap_or(key),
        "signedIn": false,
        "note": "no session yet; run `baw-rs --wallet NAME auth signin` against the resident daemon"
    }))
}

/// Files a wallet directory may contain. Anything else blocks `--delete-data`.
fn known_wallet_entries() -> [&'static str; 6] {
    [
        "session.json",
        "auth-pending",
        ".baw-rs-operations",
        ".baw-rs-resident.lock",
        ".baw-rs.sock",
        ".unused-instance",
    ]
}

fn delete_wallet_data(entry: &Entry) -> Result<(), String> {
    // A resident daemon holds this lease for its whole lifetime; refuse to delete under it.
    let lease = private_file(&entry.dir.join(".baw-rs-resident.lock"), true)?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(|_| {
        "wallet is owned by a running resident daemon; stop it before --delete-data"
    })?;
    let unexpected = std::fs::read_dir(&entry.dir)
        .map_err(|_| "cannot list wallet directory")?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|name| !known_wallet_entries().contains(&name.as_str()));
    if let Some(name) = unexpected {
        return Err(format!(
            "wallet directory contains {name}; refusing to delete it (unregister with --keep-data)"
        ));
    }
    for name in ["session.json", "auth-pending", ".baw-rs.sock"] {
        let path = entry.dir.join(name);
        if std::fs::symlink_metadata(&path).is_ok() {
            std::fs::remove_file(&path).map_err(|_| "cannot remove wallet file")?;
        }
    }
    let journal = entry.dir.join(".baw-rs-operations");
    if std::fs::symlink_metadata(&journal).is_ok() {
        std::fs::remove_dir_all(&journal).map_err(|_| "cannot remove operation journal")?;
    }
    fs2::FileExt::unlock(&lease).ok();
    drop(lease);
    std::fs::remove_file(entry.dir.join(".baw-rs-resident.lock"))
        .map_err(|_| "cannot remove wallet lease file")?;
    if std::fs::remove_file(&entry.instance_file).is_err() {
        return Err(format!(
            "cannot remove identity file {}",
            entry.instance_file.display()
        ));
    }
    std::fs::remove_dir(&entry.dir).map_err(|_| {
        format!(
            "wallet directory {} is not empty; left in place",
            entry.dir.display()
        )
    })?;
    // Per-wallet parent created by `add`; removing it is best effort.
    if let Some(parent) = entry.dir.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}

fn remove(
    registry: &Path,
    name: &str,
    keep_data: bool,
    delete_data: bool,
    confirm: bool,
) -> Result<Value, String> {
    if !valid_name(name) {
        return Err(invalid_name(name));
    }
    if keep_data && delete_data {
        return Err("choose either --keep-data (default) or --delete-data".into());
    }
    if delete_data && !confirm {
        return Err("--delete-data requires --confirm".into());
    }
    registry_dir(registry, false)?;
    let _lock = lock_registry(registry, true)?;
    let mut entries = load(registry)?;
    let entry = entries.remove(name).ok_or_else(|| {
        format!(
            "unknown wallet '{name}'; not registered in {}",
            registry.display()
        )
    })?;
    if delete_data {
        delete_wallet_data(&entry)?;
    }
    save(registry, &entries)?;
    Ok(json!({
        "removed": name,
        "walletDir": entry.dir,
        "instanceIdFile": entry.instance_file,
        "dataKept": !delete_data
    }))
}

pub(crate) fn task_control(action: TaskAction) -> Result<Control, String> {
    Ok(match action {
        TaskAction::Load { config } => {
            if !config.is_absolute() {
                return Err("task configuration path must be absolute".into());
            }
            private_dir(config.parent().ok_or("task config parent missing")?)?;
            let mut bytes = Vec::new();
            private_file(&config, false)?
                .take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read task config")?;
            if bytes.len() > 65536 {
                return Err("task configuration exceeds limit".into());
            }
            Control::Load {
                config: serde_json::from_slice(&bytes)
                    .map_err(|_| "invalid task configuration JSON")?,
            }
        }
        TaskAction::Arm { task_id, confirm } => Control::Arm { task_id, confirm },
        TaskAction::Status => Control::Status {},
        TaskAction::Disarm { task_id } => Control::Disarm { task_id },
    })
}

/// Forward a business command to the resident daemon under a resolved name.
pub async fn dispatch_named(
    registry: &Path,
    name: &str,
    argv: Vec<String>,
    require_session: bool,
) -> Result<Value, String> {
    let resolved = resolve(registry, name)?;
    if require_session && !resolved.dir.join("session.json").exists() {
        return Err(format!(
            "wallet '{name}' is not signed in; run `baw-rs --wallet {name} auth signin`"
        ));
    }
    crate::resident::send(
        &socket_path(registry),
        crate::resident::Request::Run {
            name: name.to_owned(),
            argv,
        },
    )
    .await
    .map_err(|message| clarify(message, name))
}

/// Native event task control for a named wallet.
pub async fn dispatch_task(
    registry: &Path,
    name: &str,
    action: TaskAction,
) -> Result<Value, String> {
    let control = task_control(action)?;
    resolve(registry, name)?;
    crate::resident::send(
        &socket_path(registry),
        crate::resident::Request::Native {
            id: "native-control".into(),
            name: name.to_owned(),
            command: control,
        },
    )
    .await
    .map_err(|message| clarify(message, name))
}

/// Name the wallet in the two daemon errors a caller can act on.
fn clarify(message: String, name: &str) -> String {
    match message.as_str() {
        "wallet session is missing" => {
            format!("wallet '{name}' is not signed in; run `baw-rs --wallet {name} auth signin`")
        }
        "daemon unavailable; no fallback" => format!(
            "resident daemon for wallet '{name}' is not running; start it with `baw-rs wallets serve`"
        ),
        _ => message,
    }
}

/// Management control needs the daemon too; say where it should be.
fn daemon_hint(message: String, registry: &Path) -> String {
    match message.as_str() {
        "daemon unavailable; no fallback" => format!(
            "resident daemon is not running for registry {}; start it with `baw-rs wallets serve`",
            registry.display()
        ),
        _ => message,
    }
}

pub async fn run(action: Action, registry: PathBuf) -> Result<Value, String> {
    match action {
        Action::Add {
            name,
            wallet_root,
            wallet_dir,
            instance_id_file,
        } => add(
            &registry,
            &name,
            wallet_root.as_deref(),
            wallet_dir.as_deref(),
            instance_id_file.as_deref(),
        ),
        Action::Remove {
            name,
            keep_data,
            delete_data,
            confirm,
        } => remove(&registry, &name, keep_data, delete_data, confirm),
        Action::Register {
            name,
            wallet_dir,
            instance_id_file,
        } => register(&registry, &name, &wallet_dir, &instance_id_file),
        Action::List => {
            registry_dir(&registry, false)?;
            let _lock = lock_registry(&registry, false)?;
            let entries = load(&registry)?;
            Ok(json!({"wallets":entries.iter().map(|(name,e)|json!({
                "name":name,
                "walletDir":e.dir,
                "instanceIdFile":e.instance_file,
                // Presence only: never decrypts or contacts the backend.
                "signedIn":e.dir.join("session.json").exists(),
            })).collect::<Vec<_>>()}))
        }
        Action::Serve => {
            registry_dir(&registry, false)?;
            let _lock = lock_registry(&registry, false)?;
            let entries = load(&registry)?;
            crate::resident::serve(&socket_path(&registry), entries).await
        }
        Action::Status => {
            crate::resident::send(&socket_path(&registry), crate::resident::Request::Status)
                .await
                .map_err(|message| daemon_hint(message, &registry))
        }
        Action::Stop => {
            crate::resident::send(&socket_path(&registry), crate::resident::Request::Stop)
                .await
                .map_err(|message| daemon_hint(message, &registry))
        }
        Action::Task { name, command } => {
            crate::resident::send(
                &socket_path(&registry),
                crate::resident::Request::Native {
                    id: "native-control".into(),
                    name,
                    command: task_control(command)?,
                },
            )
            .await
        }
        Action::Run { name, argv } => {
            crate::resident::send(
                &socket_path(&registry),
                crate::resident::Request::Run { name, argv },
            )
            .await
        }
    }
}
