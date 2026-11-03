//! One task, several wallets.
//!
//! A config may name the wallets it occupies (`"wallets": ["w1", "w2", ...]`). The daemon then
//! loads the same task into each member with the sender substituted per wallet, so a single CLI
//! call arms the whole group. The members share one listener through `native_events::Feeds`
//! (identical source fingerprint), which is what keeps N wallets at one WS connection and one
//! HTTP poll instead of N.
//!
//! Membership is explicit, never implicit: a wallet not listed is never touched, and every name is
//! resolved through the registry with the same checks a single-wallet command uses. A member that
//! cannot be prepared fails the whole load — a half-armed group would otherwise look armed to the
//! operator while silently sending from fewer wallets.
use super::*;
use crate::native_events::Control;
use std::collections::BTreeMap;

/// task_id -> member wallet names, in declaration order.
pub(super) type Members = std::sync::Mutex<BTreeMap<String, Vec<String>>>;

pub(super) fn new_members() -> Members {
    std::sync::Mutex::new(BTreeMap::new())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Read and validate the membership list of a raw config value.
pub(super) fn members_of(config: &Value) -> Result<Vec<String>, String> {
    let list = config
        .get("wallets")
        .and_then(Value::as_array)
        .ok_or("shared task requires a wallets array")?;
    if list.is_empty() || list.len() > 16 {
        return Err("shared task requires 1..16 wallets".into());
    }
    let mut names = Vec::with_capacity(list.len());
    for entry in list {
        let name = entry
            .as_str()
            .filter(|name| valid_name(name))
            .ok_or("shared task wallet names must be 1..64 ASCII letters/digits/-/_")?;
        if names.iter().any(|seen: &String| seen == name) {
            return Err(format!("shared task lists wallet {name} twice"));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

/// The same task, bound to one member's own sender. Everything else — source, policy, action
/// shape — stays byte-for-byte what the operator wrote, so the feed fingerprint still matches and
/// the policy still applies per wallet.
fn bind_member(config: &Value, address: &str) -> Result<Value, String> {
    let mut bound = config.clone();
    bound["wallets"] = json!([]);
    let kind = bound
        .get("action")
        .and_then(|a| a.get("kind"))
        .and_then(Value::as_str)
        .ok_or("shared task action kind missing")?;
    match kind {
        "fixed" | "words" => {
            bound["action"]["call"]["from"] = json!(address);
        }
        "route" => {
            bound["action"]["config"]["user_wallet_address"] = json!(address);
        }
        other => return Err(format!("unsupported action kind for shared task: {other}")),
    }
    Ok(bound)
}

async fn resolve(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    registry: Option<&std::path::PathBuf>,
    name: &str,
) -> Result<Arc<Wallet>, String> {
    wallet_for(wallets, name, registry).await
}

/// Load the task into every member. Fails closed: already-loaded members are disarmed again.
pub(super) async fn load(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    registry: Option<&std::path::PathBuf>,
    leader: &str,
    config: Value,
    members: &Members,
) -> Result<Value, String> {
    let names = members_of(&config)?;
    if !names.iter().any(|name| name == leader) {
        return Err(format!(
            "shared task must include the controlling wallet '{leader}' in its wallets list"
        ));
    }
    let chain = config
        .get("chain_id")
        .and_then(Value::as_str)
        .ok_or("shared task requires chain_id")?
        .to_owned();
    let task_id = config
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or("shared task requires task_id")?
        .to_owned();

    let mut loaded: Vec<(String, Arc<Wallet>)> = Vec::new();
    for name in &names {
        let attempt = async {
            let wallet = resolve(wallets, registry, name).await?;
            let address = wallet.native_chain_address(&chain).await?;
            let bound = bind_member(&config, &address)?;
            wallet
                .native_control(Control::Load { config: bound })
                .await?;
            Ok::<_, String>((name.clone(), wallet))
        }
        .await;
        match attempt {
            Ok(pair) => loaded.push(pair),
            Err(error) => {
                // No half-armed group: every member that did load is disarmed again.
                for (_, wallet) in &loaded {
                    let _ = wallet.native_control(Control::Disarm { task_id: task_id.clone() }).await;
                }
                return Err(format!("shared task load failed for {name}: {error}"));
            }
        }
    }
    members
        .lock()
        .map_err(|_| "shared task registry unavailable")?
        .insert(task_id.clone(), names.clone());
    Ok(json!({
        "task_id": task_id,
        "shared": true,
        "wallets": names,
        "status": "LOADED"
    }))
}

/// Arm every member of a loaded shared task.
pub(super) async fn arm(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    registry: Option<&std::path::PathBuf>,
    task_id: &str,
    confirm: bool,
    members: &Members,
) -> Result<Value, String> {
    let names = members
        .lock()
        .map_err(|_| "shared task registry unavailable")?
        .get(task_id)
        .cloned()
        .ok_or_else(|| format!("no shared task '{task_id}' is loaded"))?;
    let mut armed: Vec<(String, Arc<Wallet>)> = Vec::new();
    for name in &names {
        let attempt = async {
            let wallet = resolve(wallets, registry, name).await?;
            wallet
                .native_control(Control::Arm {
                    task_id: task_id.to_owned(),
                    confirm,
                })
                .await?;
            Ok::<_, String>((name.clone(), wallet))
        }
        .await;
        match attempt {
            Ok(pair) => armed.push(pair),
            Err(error) => {
                for (_, wallet) in &armed {
                    let _ = wallet
                        .native_control(Control::Disarm {
                            task_id: task_id.to_owned(),
                        })
                        .await;
                }
                return Err(format!("shared task arm failed for {name}: {error}"));
            }
        }
    }
    status(wallets, registry, task_id, members).await
}

/// Per-member status, so an operator can see which wallet is armed, which fired and which failed.
pub(super) async fn status(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    registry: Option<&std::path::PathBuf>,
    task_id: &str,
    members: &Members,
) -> Result<Value, String> {
    let names = members
        .lock()
        .map_err(|_| "shared task registry unavailable")?
        .get(task_id)
        .cloned()
        .ok_or_else(|| format!("no shared task '{task_id}' is loaded"))?;
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let row = async {
            let wallet = resolve(wallets, registry, name).await?;
            wallet.native_control(Control::Status {}).await
        }
        .await
        .unwrap_or_else(|error| json!({"status": "UNKNOWN", "error": error}));
        rows.push(json!({"wallet": name, "task": row}));
    }
    Ok(json!({
        "task_id": task_id,
        "shared": true,
        "wallets": rows,
        "count": names.len(),
    }))
}

/// Disarm every member. A member that already fired is reported as-is by `native_control`.
pub(super) async fn disarm(
    wallets: &RwLock<BTreeMap<String, Arc<Wallet>>>,
    registry: Option<&std::path::PathBuf>,
    task_id: &str,
    members: &Members,
) -> Result<Value, String> {
    let names = members
        .lock()
        .map_err(|_| "shared task registry unavailable")?
        .get(task_id)
        .cloned()
        .ok_or_else(|| format!("no shared task '{task_id}' is loaded"))?;
    let mut errors = Vec::new();
    for name in &names {
        let attempt = async {
            let wallet = resolve(wallets, registry, name).await?;
            wallet
                .native_control(Control::Disarm {
                    task_id: task_id.to_owned(),
                })
                .await
        }
        .await;
        if let Err(error) = attempt {
            errors.push(format!("{name}: {error}"));
        }
    }
    if !errors.is_empty() {
        return Err(format!("shared task disarm incomplete: {}", errors.join("; ")));
    }
    Ok(json!({"task_id": task_id, "shared": true, "status": "DISARMED", "wallets": names}))
}

/// Whether this wallet controls that shared task. Checked before fanning out, so a wallet outside
/// the group cannot arm or disarm it.
pub(super) fn is_member(members: &Members, task_id: &str, wallet: &str) -> bool {
    members
        .lock()
        .ok()
        .and_then(|m| m.get(task_id).cloned())
        .is_some_and(|names| names.iter().any(|name| name == wallet))
}

/// Which shared task a wallet belongs to, if any. Used to answer `task status` for a member.
pub(super) fn task_of(members: &Members, wallet: &str) -> Option<String> {
    members
        .lock()
        .ok()?
        .iter()
        .find(|(_, names)| names.iter().any(|name| name == wallet))
        .map(|(task_id, _)| task_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_must_be_explicit_unique_and_bounded() {
        let names = members_of(&json!({"wallets": ["w1", "w2"]})).unwrap();
        assert_eq!(names, vec!["w1".to_owned(), "w2".to_owned()]);
        for bad in [
            json!({"wallets": []}),
            json!({"wallets": ["w1", "w1"]}),
            json!({"wallets": ["w1", "bad name"]}),
            json!({"wallets": ["w1", "../escape"]}),
            json!({}),
        ] {
            assert!(members_of(&bad).is_err(), "accepted {bad}");
        }
        let too_many: Vec<String> = (0..17).map(|n| format!("w{n}")).collect();
        assert!(members_of(&json!({"wallets": too_many})).is_err());
    }

    #[test]
    fn each_member_gets_its_own_sender_and_nothing_else_changes() {
        let base = json!({"task_id":"t","chain_id":"56","policy":{"version":1},
            "action":{"kind":"words","call":{"from":"","to":"0x2222222222222222222222222222222222222222","value":"0","input_data":"0x12345678","gas_limit":100000},
            "args":[]}});
        let bound = bind_member(&base, "0x1111111111111111111111111111111111111111").unwrap();
        assert_eq!(
            bound["action"]["call"]["from"],
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(bound["wallets"], json!([]));
        // Everything the operator authorized stays identical, or the source fingerprint would
        // differ between members and they would stop sharing one listener.
        assert_eq!(bound["action"]["call"]["to"], base["action"]["call"]["to"]);
        assert_eq!(bound["policy"], base["policy"]);

        let route = json!({"action":{"kind":"route","config":{"user_wallet_address":""}}});
        let bound = bind_member(&route, "0x3333333333333333333333333333333333333333").unwrap();
        assert_eq!(
            bound["action"]["config"]["user_wallet_address"],
            "0x3333333333333333333333333333333333333333"
        );
        assert!(bind_member(&json!({"action":{"kind":"unknown"}}), "0x00").is_err());
    }

    #[test]
    fn only_listed_wallets_can_control_the_task() {
        let members = new_members();
        members
            .lock()
            .unwrap()
            .insert("t1".into(), vec!["w1".into(), "w2".into()]);
        assert!(is_member(&members, "t1", "w1"));
        assert!(!is_member(&members, "t1", "w3"));
        assert!(!is_member(&members, "t2", "w1"));
        assert_eq!(task_of(&members, "w2").as_deref(), Some("t1"));
        assert_eq!(task_of(&members, "w9"), None);
    }
}
