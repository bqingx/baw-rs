//! Durable local intent guard; never claim server idempotency or retry on ambiguity.
use serde_json::Value;
use std::path::Path;

#[derive(Clone)]
pub(crate) enum TaskAllowance {
    Prepare,
    Prepared(Value),
}

#[path = "prepared_wallet.rs"]
mod prepared_wallet;
pub(crate) use prepared_wallet::Policy as PreparedWalletPolicy;
#[cfg(test)]
#[path = "prepared_policy_tests.rs"]
mod prepared_policy_tests;

type BindingCheck = std::sync::Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
tokio::task_local! {
    static BINDING_GATE: BindingCheck;
    static TASK_GATE: (u64, std::sync::Arc<std::sync::atomic::AtomicBool>);
    static RPC_GATE: std::sync::Arc<crate::chain_rpc::SharedRpc>;
    static ALLOWANCE_GATE: TaskAllowance;
    static PREPARED_WALLET: PreparedWalletPolicy;
}

pub(crate) async fn task_scope<F: std::future::Future>(
    deadline_ms: u64,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    future: F,
) -> F::Output {
    TASK_GATE.scope((deadline_ms, cancelled), future).await
}

pub(crate) async fn binding_scope<F: std::future::Future>(
    check: BindingCheck,
    future: F,
) -> F::Output {
    BINDING_GATE.scope(check, future).await
}

pub(crate) async fn rpc_scope<F: std::future::Future>(
    rpc: std::sync::Arc<crate::chain_rpc::SharedRpc>,
    future: F,
) -> F::Output {
    RPC_GATE.scope(rpc, future).await
}

pub(crate) async fn allowance_scope<F: std::future::Future>(
    phase: TaskAllowance,
    future: F,
) -> F::Output {
    ALLOWANCE_GATE.scope(phase, future).await
}

pub(crate) fn task_allowance() -> Option<TaskAllowance> {
    ALLOWANCE_GATE.try_with(Clone::clone).ok()
}

pub(crate) async fn prepared_wallet_scope<F: std::future::Future>(
    policy: PreparedWalletPolicy,
    future: F,
) -> F::Output {
    PREPARED_WALLET.scope(policy, future).await
}
pub(crate) fn prepared_wallet() -> Option<PreparedWalletPolicy> {
    PREPARED_WALLET.try_with(Clone::clone).ok()
}
pub(crate) fn task_deadline_ms() -> Result<u64, String> {
    TASK_GATE
        .try_with(|(deadline, _)| *deadline)
        .map_err(|_| "prepared execution requires the original task deadline".into())
}
pub(crate) fn require_prepared_context() -> Result<(), String> {
    task_deadline_ms()?;
    BINDING_GATE
        .try_with(|_| ())
        .map_err(|_| "prepared execution requires the original wallet binding gate".to_owned())
}
pub(crate) fn check_prepared_task() -> Result<(), String> {
    require_prepared_context()?;
    check_task_gate()
}
pub(crate) fn check_auxiliary_read() -> Result<(), String> {
    if PREPARED_WALLET.try_with(|_| ()).is_ok() {
        return Err("auxiliary state reads forbidden on prepared task execution path".into());
    }
    Ok(())
}
pub(crate) fn check_prepared_http(method: &reqwest::Method, path: &str) -> Result<(), String> {
    if PREPARED_WALLET.try_with(|_| ()).is_err() {
        return Ok(());
    }
    require_prepared_context()?;
    if method == reqwest::Method::POST
        && matches!(
            path.strip_prefix(crate::client::API),
            Some("/contract-call/preview" | "/contract-call/execute")
        )
    {
        return Ok(());
    }
    Err("prepared task permits only preview/execute backend requests".into())
}

pub(crate) fn in_task_scope() -> bool {
    TASK_GATE.try_with(|_| ()).is_ok()
}

pub(crate) fn task_rpc() -> Option<std::sync::Arc<crate::chain_rpc::SharedRpc>> {
    RPC_GATE.try_with(std::sync::Arc::clone).ok()
}

/// Rechecked immediately before each backend request; never cancels a submitted request.
pub(crate) fn check_task_gate() -> Result<(), String> {
    TASK_GATE
        .try_with(|(deadline, cancelled)| {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst)
                || chrono::Utc::now().timestamp_millis().max(0) as u64 >= *deadline
            {
                Err::<(), String>(
                    "task cancelled or expired; no new backend request admitted".into(),
                )
            } else {
                Ok(())
            }
        })
        .unwrap_or(Ok(()))?;
    BINDING_GATE.try_with(|check| check()).unwrap_or(Ok(()))
}

pub fn require_consent(confirmed: bool, operation_id: Option<&str>) -> Result<&str, String> {
    if !confirmed {
        return Err("mutating command requires explicit --confirm and --operation-id; never auto-retry an uncertain submission".into());
    }
    let id = operation_id.ok_or("mutating command requires --operation-id")?;
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("operation-id must contain 1..64 ASCII letters/digits/-/_".into());
    }
    Ok(id)
}

pub struct OperationGuard {
    record: std::fs::File,
    metadata: Value,
    /// Held for the duration of the mutation unless `BAW_RS_CONCURRENT_WRITES=1`
    /// explicitly opts out (needed to find out what the backend does with two writes from
    /// one wallet at the same time).
    _lock: Option<std::fs::File>,
}
impl OperationGuard {
    pub fn begin(
        dir: &Path,
        id: &str,
        action: &str,
        intent: &Value,
        wallet_binding: &str,
    ) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("operation-id must contain 1..64 ASCII letters/digits/-/_".into());
        }
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(dir.components().collect::<std::path::PathBuf>())
            .map_err(|_| "cannot open wallet directory")?;
        let meta = directory
            .metadata()
            .map_err(|_| "cannot inspect wallet directory")?;
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o7777 != 0o700 {
            return Err("wallet directory must be owned and private (0700)".into());
        }
        fn opened_at(dir: i32, name: &std::ffi::CStr, flags: i32) -> Result<std::fs::File, String> {
            let fd = unsafe {
                libc::openat(
                    dir,
                    name.as_ptr(),
                    flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                    0o600,
                )
            };
            if fd < 0 {
                return Err("cannot create private operation file; an existing operation-id is never resubmitted".into());
            }
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            let m = file
                .metadata()
                .map_err(|_| "cannot inspect operation file")?;
            if !m.is_file()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o7777 != 0o600
                || m.nlink() != 1
            {
                return Err("operation file must be owned, private and regular".into());
            }
            Ok(file)
        }
        let lock = opened_at(
            directory.as_raw_fd(),
            c".baw-rs-write.lock",
            libc::O_RDWR | libc::O_CREAT,
        )?;
        let lock = if std::env::var("BAW_RS_CONCURRENT_WRITES").is_ok_and(|v| v == "1") {
            None
        } else {
            fs2::FileExt::try_lock_exclusive(&lock)
                .map_err(|_| "another wallet mutation is running")?;
            Some(lock)
        };
        let created =
            unsafe { libc::mkdirat(directory.as_raw_fd(), c".baw-rs-operations".as_ptr(), 0o700) };
        if created < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err("cannot create operation journal".into());
        }
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                c".baw-rs-operations".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("cannot open operation journal".into());
        }
        let journal = unsafe { std::fs::File::from_raw_fd(fd) };
        let m = journal
            .metadata()
            .map_err(|_| "cannot inspect operation journal")?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
            return Err("operation journal must be owned and private".into());
        }
        let name =
            std::ffi::CString::new(format!("{id}.json")).map_err(|_| "invalid operation-id")?;
        let mut record = opened_at(
            journal.as_raw_fd(),
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        let encoded = serde_json::to_vec(
            &serde_json::json!({"action":action,"intent":intent,"wallet":wallet_binding}),
        )
        .map_err(|_| "cannot fingerprint operation")?;
        let hash = format!("{:x}", Sha256::digest(encoded));
        let metadata = serde_json::json!({"state":"SUBMITTING","action":action,"intentSha256":hash,"createdAtMs":chrono::Utc::now().timestamp_millis()});
        record
            .write_all(metadata.to_string().as_bytes())
            .map_err(|_| "cannot persist operation intent")?;
        record
            .sync_all()
            .map_err(|_| "cannot sync operation intent")?;
        journal
            .sync_all()
            .map_err(|_| "cannot sync operation journal")?;
        directory
            .sync_all()
            .map_err(|_| "cannot sync wallet directory")?;
        Ok(Self {
            record,
            metadata,
            _lock: lock,
        })
    }
    /// Persist a multi-transaction stage while keeping this command's exclusive wallet lock.
    /// Only allowlisted recovery identifiers are retained; never arbitrary backend data.
    pub(crate) fn checkpoint(&mut self, stage: &str, evidence: &Value) -> Result<(), String> {
        if self._lock.is_none() {
            return Err("multi-stage operation requires the exclusive wallet write lock".into());
        }
        if !matches!(
            stage,
            "approval_submitting"
                | "approval_broadcast"
                | "approval_confirmed"
                | "approval_reverted"
                | "approval_uncertain"
                | "swap_submitting"
                | "swap_broadcast"
        ) {
            return Err("invalid operation stage".into());
        }
        if !self.metadata["progress"].is_object() {
            self.metadata["progress"] = serde_json::json!({});
        }
        self.metadata["progress"]["stage"] = serde_json::json!(stage);
        for key in ["approvalTxHash", "approvalOrderId", "swapTxHash"] {
            if let Some(value) = evidence.get(key).and_then(Value::as_str).filter(|v| {
                !v.is_empty() && v.len() <= 256 && v.bytes().all(|b| b.is_ascii_graphic())
            }) {
                self.metadata["progress"][key] = serde_json::json!(value);
            }
        }
        self.metadata["updatedAtMs"] = serde_json::json!(chrono::Utc::now().timestamp_millis());
        self.persist()
    }

    fn persist(&mut self) -> Result<(), String> {
        use std::io::{Seek, SeekFrom, Write};
        self.record
            .seek(SeekFrom::Start(0))
            .map_err(|_| "journal update failed; reconcile before any retry")?;
        self.record
            .set_len(0)
            .map_err(|_| "journal update failed; reconcile before any retry")?;
        self.record
            .write_all(self.metadata.to_string().as_bytes())
            .map_err(|_| "journal update failed; reconcile before any retry")?;
        self.record
            .sync_all()
            .map_err(|_| "journal sync failed; reconcile before any retry".to_string())
    }

    pub fn finish(&mut self, result: &Result<Value, String>) -> Result<(), String> {
        self.metadata["state"] = Value::String(
            if result.is_ok() {
                "RESPONSE_RECEIVED"
            } else {
                "FAILED_OR_UNKNOWN"
            }
            .into(),
        );
        self.metadata["updatedAtMs"] = serde_json::json!(chrono::Utc::now().timestamp_millis());
        if let Ok(data) = result {
            let mut recovery = serde_json::Map::new();
            for key in ["orderId", "clientOrderId", "txHash", "status"] {
                if let Some(value) = data
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|v| v.len() <= 256 && v.bytes().all(|b| b.is_ascii_graphic()))
                {
                    recovery.insert(key.into(), Value::String(value.into()));
                }
            }
            self.metadata["recovery"] = Value::Object(recovery);
        }
        self.persist()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn approval_checkpoint_survives_failure_and_retains_only_recovery_evidence() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut guard = OperationGuard::begin(
            dir.path(),
            "staged-1",
            "market-order.swap",
            &serde_json::json!({}),
            "synthetic",
        )
        .unwrap();
        let hash = format!("0x{}", "b".repeat(64));
        guard
            .checkpoint("approval_submitting", &serde_json::json!({}))
            .unwrap();
        guard.checkpoint("approval_broadcast", &serde_json::json!({"approvalTxHash":hash,"signature":"sensitive-signature","api_key":"sensitive-key"})).unwrap();
        assert!(OperationGuard::begin(
            dir.path(),
            "parallel",
            "swap",
            &serde_json::json!({}),
            "synthetic"
        )
        .is_err());
        guard
            .finish(&Err("sensitive-backend-error".into()))
            .unwrap();
        let text =
            std::fs::read_to_string(dir.path().join(".baw-rs-operations/staged-1.json")).unwrap();
        assert!(!text.contains("sensitive"));
        let record: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(record["progress"]["approvalTxHash"], hash);
        assert_eq!(record["progress"]["stage"], "approval_broadcast");
        assert_eq!(record["state"], "FAILED_OR_UNKNOWN");
        assert!(guard
            .checkpoint("unrecognized-stage", &serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn requires_both_explicit_confirmation_and_unique_named_intent() {
        assert_eq!(
            require_consent(true, Some("intent-123")).unwrap(),
            "intent-123"
        );
        for (confirmed, id) in [
            (false, Some("intent-123")),
            (true, None),
            (true, Some("")),
            (true, Some("../unsafe")),
        ] {
            assert!(require_consent(confirmed, id).is_err());
        }
    }
    #[test]
    fn rejects_trailing_slash_directory_symlink_without_creating_files() {
        let owner = tempfile::tempdir().unwrap();
        let outer = tempfile::tempdir().unwrap();
        std::fs::set_permissions(owner.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = outer.path().join("wallet-link");
        std::os::unix::fs::symlink(owner.path(), &link).unwrap();
        let slash = std::path::PathBuf::from(format!("{}/", link.display()));
        assert!(
            OperationGuard::begin(&slash, "op", "swap", &serde_json::json!({}), "wallet").is_err()
        );
        assert_eq!(std::fs::read_dir(owner.path()).unwrap().count(), 0);
    }
    #[test]
    fn terminal_record_keeps_recovery_ids_but_never_signature_or_backend_message() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut guard = OperationGuard::begin(
            dir.path(),
            "sig-1",
            "sign-message",
            &serde_json::json!({}),
            "synthetic-wallet",
        )
        .unwrap();
        guard.finish(&Ok(serde_json::json!({"orderId":"recover-id","status":"COMPLETED","signature":"sensitive-signature","message":"sensitive-backend"}))).unwrap();
        drop(guard);
        let text =
            std::fs::read_to_string(dir.path().join(".baw-rs-operations/sig-1.json")).unwrap();
        assert!(text.contains("recover-id"));
        assert!(!text.contains("sensitive-"));
        let row: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(row["state"], "RESPONSE_RECEIVED");
        assert!(OperationGuard::begin(
            dir.path(),
            "sig-1",
            "sign-message",
            &serde_json::json!({}),
            "synthetic-wallet"
        )
        .is_err());
    }
    #[test]
    fn writes_intent_before_return_and_refuses_duplicate_even_after_drop() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let guard = OperationGuard::begin(
            dir.path(),
            "intent-1",
            "swap",
            &serde_json::json!({"amount":"1"}),
            "synthetic-wallet",
        )
        .unwrap();
        let entries = std::fs::read_dir(dir.path().join(".baw-rs-operations"))
            .unwrap()
            .count();
        assert_eq!(entries, 1);
        drop(guard);
        assert!(OperationGuard::begin(
            dir.path(),
            "intent-1",
            "swap",
            &serde_json::json!({"amount":"2"}),
            "synthetic-wallet"
        )
        .is_err());
    }
}
