//! Read-only view of the durable intent journal.
//!
//! `OperationGuard` writes `.baw-rs-operations/<id>.json` before a mutation leaves the box and
//! rewrites it once the backend answers. A crash or a transport failure therefore leaves a real
//! record of "we may have submitted this". This module turns that record into a scriptable
//! verdict so callers never have to guess, and never resubmit on a guess.
//!
//! Read-only by construction: it opens the directory and files `O_NOFOLLOW`, requires 0700/0600
//! ownership, and never creates, truncates or renames anything.
use clap::Args;
use serde_json::{json, Value};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

#[derive(Args, Debug, Clone)]
pub struct OperationArgs {
    /// Populated from the global --operation-id by dispatch.
    #[arg(skip)]
    pub id: Option<String>,
    /// List every operation id recorded for this wallet, newest last.
    #[arg(long)]
    pub list: bool,
}

const MAX_FILES: usize = 128;
const MAX_RECORD: u64 = 64 * 1024;

/// Verdict for one operation id. `retrySafe` is only ever true when no request could have been
/// sent, which the journal itself cannot prove — so it stays false for every recorded state.
fn verdict(record: &Value) -> (&'static str, bool) {
    match record.get("state").and_then(Value::as_str) {
        Some("RESPONSE_RECEIVED") => ("RESPONSE_RECEIVED", false),
        Some("FAILED_OR_UNKNOWN") => ("FAILED_OR_UNKNOWN", false),
        // The guard is written before the request and rewritten after the response. A record
        // still saying SUBMITTING means the process died between those two points, or the
        // response never arrived: the submission state is genuinely unknown.
        Some("SUBMITTING") => ("UNCERTAIN", false),
        _ => ("UNRECOGNISED", false),
    }
}

fn next_step(state: &str) -> &'static str {
    match state {
        "RESPONSE_RECEIVED" => {
            "check the recovery txHash/orderId with `wallet tx-history` or the explorer; do not resubmit with a new operation-id"
        }
        "FAILED_OR_UNKNOWN" => {
            "the backend rejected or the transport failed; confirm no on-chain effect before retrying under a new operation-id"
        }
        "UNCERTAIN" => {
            "process ended mid-flight; reconcile balances/tx-history first and treat the submission as possibly live"
        }
        _ => "journal entry is not a record this build understands; inspect it manually",
    }
}

fn open_private_dir(path: &Path) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "cannot open wallet directory")?;
    let meta = file
        .metadata()
        .map_err(|_| "cannot inspect wallet directory")?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o7777 != 0o700 {
        return Err("wallet directory must be owned and private (0700)".into());
    }
    Ok(file)
}

fn read_record(dir: &Path, id: &str) -> Result<Value, String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("operation-id must contain 1..64 ASCII letters/digits/-/_".into());
    }
    let wallet = open_private_dir(dir)?;
    let journal = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(dir.join(".baw-rs-operations"))
        .map_err(|_| format!("no operation journal entry for '{id}'"))?;
    let meta = journal
        .metadata()
        .map_err(|_| "cannot inspect operation journal")?;
    if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o7777 != 0o700 {
        return Err("operation journal must be owned and private".into());
    }
    let name = std::ffi::CString::new(format!("{id}.json")).map_err(|_| "invalid operation-id")?;
    let fd = unsafe {
        libc::openat(
            journal.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(format!("no operation journal entry for '{id}'"));
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let m = file
        .metadata()
        .map_err(|_| "cannot inspect operation entry")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
    {
        return Err("operation entry must be owned, private and regular".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read operation entry")?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err("operation entry too large".into());
    }
    let mut record: Value =
        serde_json::from_slice(&bytes).map_err(|_| "operation entry is not valid JSON")?;
    if !record.is_object() {
        return Err("operation entry must be a JSON object".into());
    }
    let (state, retry_safe) = verdict(&record);
    record["verdict"] = json!(state);
    record["retrySafe"] = json!(retry_safe);
    record["nextStep"] = json!(next_step(state));
    drop(wallet);
    Ok(record)
}

/// Operation ids present in the journal, newest first. Names only: no metadata is read, so a
/// single damaged entry cannot hide the rest.
fn list_ids(dir: &Path) -> Result<Vec<String>, String> {
    let _wallet = open_private_dir(dir)?;
    let entries = match std::fs::read_dir(dir.join(".baw-rs-operations")) {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };
    let mut ids = Vec::new();
    for entry in entries.flatten().take(MAX_FILES * 4) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if id.len() <= 64
            && !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    ids.truncate(MAX_FILES);
    Ok(ids)
}

pub fn run(dir: &Path, args: &OperationArgs) -> Result<Value, String> {
    match (args.id.as_deref(), args.list) {
        (Some(_), true) => Err("use either --operation-id or --list, not both".into()),
        (Some(id), false) => Ok(read_record(dir, id)?),
        (None, true) => {
            let ids = list_ids(dir)?;
            Ok(json!({"operations": ids, "count": ids.len()}))
        }
        (None, false) => Err("pass --operation-id <ID> or --list".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn wallet() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn write(dir: &Path, id: &str, body: &str) {
        let journal = dir.join(".baw-rs-operations");
        std::fs::create_dir_all(&journal).unwrap();
        std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = journal.join(format!("{id}.json"));
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn verdict_with(dir: &Path, id: &str) -> (String, bool) {
        let record = read_record(dir, id).unwrap();
        (
            record["verdict"].as_str().unwrap().to_owned(),
            record["retrySafe"].as_bool().unwrap(),
        )
    }

    #[test]
    fn every_recorded_state_is_scriptable_and_never_retry_safe() {
        let dir = wallet();
        for (id, body, expected) in [
            ("a", r#"{"state":"SUBMITTING"}"#, "UNCERTAIN"),
            ("b", r#"{"state":"RESPONSE_RECEIVED"}"#, "RESPONSE_RECEIVED"),
            ("c", r#"{"state":"FAILED_OR_UNKNOWN"}"#, "FAILED_OR_UNKNOWN"),
            ("d", r#"{"state":"SOMETHING_NEW"}"#, "UNRECOGNISED"),
        ] {
            write(dir.path(), id, body);
            let (state, retry_safe) = verdict_with(dir.path(), id);
            assert_eq!(state, expected);
            assert!(!retry_safe, "{id} must never claim a retry is safe");
        }
    }

    #[test]
    fn non_object_journal_entries_are_errors_not_panics() {
        let dir = wallet();
        write(dir.path(), "invalid", "[]");
        assert!(run(
            dir.path(),
            &OperationArgs {
                id: Some("invalid".into()),
                list: false
            }
        )
        .is_err());
    }

    #[test]
    fn recovery_fields_survive_but_arbitrary_fields_are_not_invented() {
        let dir = wallet();
        write(
            dir.path(),
            "op-1",
            r#"{"state":"RESPONSE_RECEIVED","action":"contract-call.execute","recovery":{"txHash":"0xabc","status":"BROADCASTED"}}"#,
        );
        let record = read_record(dir.path(), "op-1").unwrap();
        assert_eq!(record["recovery"]["txHash"], "0xabc");
        assert!(record["nextStep"]
            .as_str()
            .unwrap()
            .contains("do not resubmit"));
        assert!(record.get("private").is_none() || record["private"].is_null());
    }

    #[test]
    fn missing_ids_and_unsafe_names_are_errors_not_empty_results() {
        let dir = wallet();
        assert!(read_record(dir.path(), "absent").is_err());
        for bad in ["", "../escape", "a/b", "spaces here"] {
            assert!(read_record(dir.path(), bad).is_err(), "{bad}");
        }
        assert_eq!(list_ids(dir.path()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn symlinked_entry_is_refused() {
        let dir = wallet();
        write(dir.path(), "real", r#"{"state":"SUBMITTING"}"#);
        std::os::unix::fs::symlink(
            dir.path().join(".baw-rs-operations/real.json"),
            dir.path().join(".baw-rs-operations/link.json"),
        )
        .unwrap();
        assert!(read_record(dir.path(), "link").is_err());
        assert!(read_record(dir.path(), "real").is_ok());
    }

    #[test]
    fn listing_and_lookup_are_mutually_exclusive() {
        let dir = wallet();
        let both = OperationArgs {
            id: Some("a".into()),
            list: true,
        };
        let neither = OperationArgs {
            id: None,
            list: false,
        };
        let listing = OperationArgs {
            id: None,
            list: true,
        };
        assert!(run(dir.path(), &both).is_err());
        assert!(run(dir.path(), &neither).is_err());
        assert_eq!(run(dir.path(), &listing).unwrap()["count"], 0);
    }

    #[test]
    fn world_readable_journal_is_refused() {
        let dir = wallet();
        write(dir.path(), "op", r#"{"state":"SUBMITTING"}"#);
        std::fs::set_permissions(
            dir.path().join(".baw-rs-operations"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(read_record(dir.path(), "op").is_err());
    }
}
