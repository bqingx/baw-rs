//! Named-wallet control plane. Registry contains paths and filesystem identities, never keys.
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "baw-rs vaults",
    about = "Native named wallets; no implicit wallet or fallback"
)]
pub struct Args {
    #[arg(long)]
    pub registry: PathBuf,
    #[command(subcommand)]
    pub command: Action,
}
#[derive(Subcommand)]
pub enum Action {
    Register {
        name: String,
        #[arg(long)]
        wallet_dir: PathBuf,
        #[arg(long)]
        instance_id_file: PathBuf,
    },
    List,
    Status,
    Serve,
    Stop,
    Run {
        name: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
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
type Registry = BTreeMap<String, Entry>;
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
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
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| "cannot create registry directory")?;
    }
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
fn register(path: &Path, name: &str, dir: &Path, key: &Path) -> Result<Value, String> {
    if !valid_name(name) {
        return Err(
            "wallet name must contain 1..64 ASCII letters, digits, hyphens or underscores".into(),
        );
    }
    registry_dir(path, true)?;
    let _lock = lock_registry(path, true)?;
    let mut entries = load(path)?;
    if entries.contains_key(name) || entries.len() >= 128 {
        return Err("wallet name already registered or registry full".into());
    }
    let dir = dir
        .canonicalize()
        .map_err(|_| "wallet directory must exist")?;
    private_dir(&dir)?;
    // Validate the original path before canonicalizing: instance-file symlinks are forbidden.
    let _ = crate::session::resolve_instance(String::new(), Some(key))?;
    let key = key
        .canonicalize()
        .map_err(|_| "instance-id file must exist")?;
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
    let bytes = serde_json::to_vec_pretty(&entries).map_err(|_| "cannot encode registry")?;
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
    Ok(json!({"registered":name}))
}
pub async fn run(args: Args) -> Result<Value, String> {
    match args.command {
        Action::Register {
            name,
            wallet_dir,
            instance_id_file,
        } => register(&args.registry, &name, &wallet_dir, &instance_id_file),
        Action::List => {
            registry_dir(&args.registry, false)?;
            let _lock = lock_registry(&args.registry, false)?;
            let entries = load(&args.registry)?;
            Ok(
                json!({"wallets":entries.iter().map(|(name,e)|json!({"name":name,"walletDir":e.dir,"instanceIdFile":e.instance_file})).collect::<Vec<_>>()}),
            )
        }
        Action::Serve => {
            registry_dir(&args.registry, false)?;
            let _lock = lock_registry(&args.registry, false)?;
            let entries = load(&args.registry)?;
            crate::resident::serve(&args.registry.join("daemon.sock"), entries).await
        }
        Action::Status => {
            crate::resident::send(
                &args.registry.join("daemon.sock"),
                crate::resident::Request::Status,
            )
            .await
        }
        Action::Stop => {
            crate::resident::send(
                &args.registry.join("daemon.sock"),
                crate::resident::Request::Stop,
            )
            .await
        }
        Action::Run { name, argv } => {
            crate::resident::send(
                &args.registry.join("daemon.sock"),
                crate::resident::Request::Run { name, argv },
            )
            .await
        }
    }
}
