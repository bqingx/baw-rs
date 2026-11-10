//! Explicit manual inspection. Default is local only; refresh has independent disposable
//! clients and never changes resident financial caches, session files or task authorization.
use crate::core::client::API;
use crate::core::session;
use crate::daemon::resident::{file_stamp, session_stamp, Daemon};
use crate::daemon::wallets::Entry;
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
const MAX_REFRESH: usize = 16;

fn digits(value: Option<&Value>) -> Value {
    value
        .and_then(|v| match v {
            Value::String(s)
                if !s.is_empty()
                    && s.len() <= 78
                    && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                    && s.parse::<bigdecimal::BigDecimal>().is_ok() =>
            {
                Some(Value::String(s.clone()))
            }
            Value::Number(n) => {
                let text = n.to_string();
                if text.len() <= 78 && text.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                    Some(Value::String(text))
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap_or(Value::Null)
}
fn journal(entry: &Entry, resolved: &std::collections::BTreeSet<String>) -> Value {
    let path = entry.dir.join(".baw-rs-operations");
    if std::fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return json!({"readStatus":"missing","complete":true,"operations":[]});
    }
    if crate::util::fsx::private_dir(&path).is_err() {
        return json!({"readStatus":"unavailable","complete":false,"operations":[]});
    }
    let Ok(files) = std::fs::read_dir(path) else {
        return json!({"readStatus":"unavailable","complete":false,"operations":[]});
    };
    let mut ids = Vec::new();
    let mut complete = true;
    for (index, file) in files.enumerate() {
        if index >= 128 {
            complete = false;
            break;
        }
        let Ok(file) = file else {
            complete = false;
            continue;
        };
        let name = file.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            .filter(|id| crate::util::validate::operation_id(id))
        else {
            complete = false;
            continue;
        };
        ids.push(id.to_owned());
    }
    ids.sort();
    let mut rows = Vec::new();
    for id in ids.iter().map(String::as_str) {
        if resolved.contains(id) {
            continue;
        }
        let Ok(record) = crate::exec::operations::run(
            &entry.dir,
            &crate::exec::operations::OperationArgs {
                id: Some(id.into()),
                list: false,
            },
        ) else {
            complete = false;
            rows.push(json!({"id":id,"state":"UNAVAILABLE","retrySafe":false}));
            continue;
        };
        let verdict = record["verdict"].as_str().unwrap_or("UNRECOGNISED");
        let status = record.pointer("/recovery/status").and_then(Value::as_str);
        let hash = record
            .pointer("/recovery/txHash")
            .and_then(Value::as_str)
            .filter(|s| {
                s.len() == 66
                    && s.starts_with("0x")
                    && s[2..].bytes().all(|b| b.is_ascii_hexdigit())
            });
        if verdict == "RESPONSE_RECEIVED"
            && record["action"] == "auth"
            && hash.is_none()
            && !matches!(
                status,
                Some("UNCERTAIN" | "PENDING_CONFIRMATION" | "FAILED" | "SUBMIT_FAIL")
            )
        {
            continue;
        }
        rows.push(json!({"id":id,"state":match verdict {"UNCERTAIN"|"FAILED_OR_UNKNOWN"|"UNRECOGNISED"=>verdict,_=>"REQUIRES_VERIFICATION"},"txHash":hash,"retrySafe":false,"chainQueried":false}));
    }
    json!({"readStatus":if complete {"ok"}else{"partial"},"complete":complete,"operations":rows})
}
async fn refresh(daemon: Arc<Daemon>, name: String, entry: Entry) -> Value {
    let failure = |reason: &str| json!({"readStatus":reason,"connectionStatus":"UNKNOWN"});
    if !daemon.scheduler.inspection_quiet() {
        return failure("skipped_active_or_interrupted_task");
    }
    let wallet = match daemon.wallets.try_read() {
        Ok(wallets) => wallets.get(&name).cloned(),
        Err(_) => return failure("skipped_wallet_table_busy"),
    };
    let Some(wallet) = wallet else {
        return failure("skipped_wallet_not_loaded");
    };
    if wallet.entry.device != entry.device
        || wallet.entry.inode != entry.inode
        || wallet.entry.instance_file != entry.instance_file
    {
        return failure("registration_changed");
    }
    let Ok(_wallet_reservation) = wallet.state.try_lock() else {
        return failure("skipped_wallet_busy");
    };
    let Some(session_before) = session_stamp(&entry.dir) else {
        return failure("no_safe_session");
    };
    let Some(identity_before) = file_stamp(&entry.instance_file) else {
        return failure("no_safe_identity");
    };
    let load_entry = entry.clone();
    let credentials = tokio::task::spawn_blocking(move || {
        load_entry.check()?;
        let identity = zeroize::Zeroizing::new(session::resolve_instance(
            String::new(),
            Some(&load_entry.instance_file),
        )?);
        session::load(&load_entry.dir, &identity)
    })
    .await;
    let Ok(Ok(credentials)) = credentials else {
        return failure("credentials_unavailable");
    };
    if credentials.session_id.is_empty() {
        return failure("no_session");
    }
    if session_stamp(&entry.dir) != Some(session_before)
        || file_stamp(&entry.instance_file) != Some(identity_before)
    {
        return failure("identity_or_session_changed");
    }
    let Ok(mut client) = wallet.build(credentials) else {
        return failure("client_unavailable");
    };
    let original = client.credentials().session_id.clone();
    if !daemon.scheduler.inspection_quiet() {
        return failure("skipped_active_or_busy");
    }
    let login = match client
        .request(Method::GET, &format!("{API}/login/query"), None)
        .await
    {
        Ok(value) => value,
        Err(_) => return failure("backend_read_failed"),
    };
    let status = match (
        login["connectionStatus"].as_str(),
        login["walletCreateStatus"].as_str(),
    ) {
        (Some("CONNECTED"), Some("CREATED")) => "CONNECTED",
        (Some("CONNECTED"), Some("CREATING")) => "CREATING",
        (Some("UNCONNECTED" | "DISCONNECTED" | "NO"), _) => "UNCONNECTED",
        _ => "UNKNOWN",
    };
    let mut row = json!({"readStatus":"partial","connectionStatus":status,"sessionExpireTimeAsReported":digits(login.get("sessionExpireTime")),"sessionIdleExpireTimeAsReported":digits(login.get("sessionIdleExpireTime")),"developerMode":{"enabled":null,"expiresAtUnixSeconds":null,"expired":null},"sessionRotationObserved":client.credentials().session_id.as_str()!=original.as_str(),"sessionPersisted":false});
    if status == "CONNECTED" {
        if !daemon.scheduler.inspection_quiet() {
            row["readStatus"] = json!("partial_task_or_wallet_became_busy");
        } else if let Ok(settings) = client
            .request(Method::GET, &format!("{API}/settings/query"), None)
            .await
        {
            let expiry = settings.pointer("/devMode/expiresAt").and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            });
            row["developerMode"] = json!({"enabled":settings.pointer("/devMode/enabled").and_then(Value::as_bool),"expiresAtUnixSeconds":expiry,"expired":expiry.map(|t|t<=chrono::Utc::now().timestamp().max(0)as u64)});
            row["dailyLimitUsdAsReported"] = digits(settings.get("dailyLimit"));
            row["readStatus"] = json!("ok");
        }
    }
    if session_stamp(&entry.dir) != Some(session_before)
        || file_stamp(&entry.instance_file) != Some(identity_before)
    {
        return failure("identity_or_session_changed");
    }
    row["sessionRotationObserved"] = json!(
        !client.credentials().session_id.is_empty()
            && client.credentials().session_id.as_str() != original.as_str()
    );
    if client.credentials().session_id.is_empty() {
        row["connectionStatus"] = json!("UNKNOWN");
        row["readStatus"] = json!("backend_session_rejected");
    } else if row["sessionRotationObserved"] == true {
        row["observedConnectionStatus"] = row["connectionStatus"].clone();
        row["connectionStatus"] = json!("UNKNOWN");
        row["readStatus"] = json!("session_rotation_not_persisted");
        row["warning"]=json!("server changed the disposable query session; stored/resident session validity is unknown, no update or renewal was persisted");
    }
    row["observedAtUnixMs"] = json!(chrono::Utc::now().timestamp_millis());
    row
}
pub(super) async fn run(daemon: &Arc<Daemon>, request_refresh: bool) -> Result<Value, String> {
    let registry = daemon.registry.as_path();
    let entries = crate::daemon::wallets::reload_entries(registry)?;
    let _inspection_reservation = if request_refresh {
        daemon.scheduler.admit_inspection()
    } else {
        None
    };
    let inspection = daemon.scheduler.inspect_local(registry).await?;
    let loaded = daemon
        .wallets
        .read()
        .await
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut rows = Vec::new();
    let mut jobs = tokio::task::JoinSet::new();
    let permits = Arc::new(Semaphore::new(4));
    for (index, (name, entry)) in entries.into_iter().enumerate() {
        let snapshot_entry = entry.clone();
        let known = inspection
            .resolved_operations
            .get(&name)
            .cloned()
            .unwrap_or_default();
        let local=tokio::task::spawn_blocking(move||{
            if snapshot_entry.check().is_err() {return json!({"readStatus":"entry_unavailable"});}
            json!({"readStatus":"ok","sessionFilePresent":session_stamp(&snapshot_entry.dir).is_some(),"identityFileSafe":file_stamp(&snapshot_entry.instance_file).is_some(),"unresolvedOperations":journal(&snapshot_entry,&known)})
        }).await.map_err(|_|"wallet inspection worker failed")?;
        let skip = if !request_refresh {
            "not_requested"
        } else if !inspection.quiet || _inspection_reservation.is_none() {
            "skipped_active_or_interrupted_task"
        } else if index >= MAX_REFRESH {
            "skipped_refresh_limit"
        } else {
            "pending"
        };
        let position = rows.len();
        rows.push(json!({"name":name,"loaded":loaded.contains(&name),"local":local,"remote":{"readStatus":skip,"connectionStatus":"NOT_REFRESHED"}}));
        if skip == "pending" {
            let daemon = daemon.clone();
            let permits = permits.clone();
            jobs.spawn(async move {
                let _permit = permits.acquire_owned().await;
                let value =
                    tokio::time::timeout(Duration::from_secs(6), refresh(daemon, name, entry))
                        .await
                        .unwrap_or_else(
                            |_| json!({"readStatus":"timeout","connectionStatus":"UNKNOWN"}),
                        );
                (position, value)
            });
        }
    }
    while let Some(result) = jobs.join_next().await {
        if let Ok((index, value)) = result {
            rows[index]["remote"] = value;
        }
    }
    Ok(
        json!({"wallets":rows,"tasks":inspection.tasks,"tasksComplete":inspection.complete,"notifications":daemon.scheduler.notification_status(),"manualRefreshRequested":request_refresh,"snapshotConsistency":"per_item_not_atomic","snapshotAtUnixMs":chrono::Utc::now().timestamp_millis(),"backgroundPolling":false,"sessionFilesWritten":false,"chainQueries":0,"refreshLimits":{"wallets":MAX_REFRESH,"concurrency":4,"maxGetRequestsPerWallet":2,"timeoutSecondsPerWallet":6},"limitations":["local session presence is not a connected-session proof; remote state is unknown unless explicitly refreshed","login expiry fields retain provider units; no unit guess or session renewal","journal entries with a hash still require chain verification unless a bound local Task confirmed that operation","manual reads are not an atomic snapshot and do not authorize execution"]}),
    )
}
