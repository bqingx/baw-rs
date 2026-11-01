use super::native_events::*;
use super::*;
#[test]
fn native_task_cli_routes_through_existing_registry() {
    let parsed = crate::cli::Cli::try_parse_from([
        "baw-rs",
        "--registry",
        "/nonexistent",
        "wallets",
        "task",
        "alice",
        "status",
    ]);
    assert!(parsed.is_ok(), "native CLI unavailable");
    assert!(crate::cli::Cli::try_parse_from([
        "baw-rs",
        "--registry",
        "/nonexistent",
        "wallets",
        "task",
        "alice",
        "arm",
        "claim-1",
        "--confirm"
    ])
    .is_ok());
    // The flattened form names the wallet once, as a global flag.
    let parsed = crate::cli::Cli::try_parse_from([
        "baw-rs",
        "--wallet",
        "alice",
        "task",
        "arm",
        "claim-1",
        "--confirm",
    ]);
    assert!(parsed.is_ok(), "flattened task form unavailable");
}
#[test]
fn native_control_is_typed_and_rejects_extra_fields() {
    let r = json!({"op":"native","id":"n","name":"alice","command":{"action":"status"}});
    assert!(
        serde_json::from_value::<Request>(r.clone()).is_ok(),
        "native control missing"
    );
    let mut bad = r;
    bad["command"]["shell"] = json!("bad");
    assert!(serde_json::from_value::<Request>(bad).is_err());
}
pub(super) fn config() -> Value {
    json!({"task_id":"claim-1","valid_for_ms":60000,
    "chain_id":"56","rpc_url":"http://127.0.0.1:1","ws_url":"ws://127.0.0.1:1",
    "filter":{"address":"0x2222222222222222222222222222222222222222","topics":[format!("0x{}","11".repeat(32))]},
    "start_block":1,"confirmation_depth":0,"max_backfill_blocks":100,"chunk_blocks":10,"queue_capacity":4,
    "action":{"kind":"fixed","call":{"binance_chain_id":"56","from":"0x1111111111111111111111111111111111111111","to":"0x2222222222222222222222222222222222222222","value":"0","input_data":"0x12345678","gas_limit":100000}},
    "policy":{"version":1,"chainId":"56","to":"0x2222222222222222222222222222222222222222","valueWei":"0"}})
}
#[test]
fn native_duration_is_required_positive_uncapped_and_rejects_absolute_deadlines() {
    let mut v = config();
    v["valid_for_ms"] = json!(86_400_000);
    assert!(
        Config::parse(v.clone()).is_ok(),
        "explicit day-long duration rejected"
    );
    for invalid in [
        json!(0),
        json!(null),
        json!("forever"),
        json!(-1),
        json!(u64::MAX),
    ] {
        v["valid_for_ms"] = invalid;
        assert!(Config::parse(v.clone()).is_err());
    }
    v.as_object_mut().unwrap().remove("valid_for_ms");
    assert!(Config::parse(v.clone()).is_err());
    v["deadline_ms"] = json!(chrono::Utc::now().timestamp_millis() + 60000);
    let error = Config::parse(v.clone()).err().unwrap();
    assert!(
        error.contains("valid_for_ms") && error.contains("deadline_ms"),
        "migration message unclear: {error}"
    );
    v["valid_for_ms"] = json!(60000);
    assert!(
        Config::parse(v).is_err(),
        "mixed absolute/relative config accepted"
    );
}
pub(super) fn pending_config(subscription: &str) -> Value {
    let mut v = config();
    for key in [
        "filter",
        "start_block",
        "confirmation_depth",
        "max_backfill_blocks",
        "chunk_blocks",
    ] {
        v.as_object_mut().unwrap().remove(key);
    }
    v["source"] = json!({"kind":"pending", "subscription":subscription,
        "from":"0x3333333333333333333333333333333333333333",
        "to":"0x2222222222222222222222222222222222222222", "selector":"0xabcdef01",
        "args":[{"index":0,"abi_type":"uint256","equals":format!("0x{:064x}",17)}]});
    v
}
pub(super) fn pending_transaction(byte: u8) -> Value {
    json!({"hash":format!("0x{}",format!("{byte:02x}").repeat(32)),
        "from":"0x3333333333333333333333333333333333333333",
        "to":"0x2222222222222222222222222222222222222222", "input":format!("0xabcdef01{:064x}",17),
        "blockHash":null,"blockNumber":null,"transactionIndex":null,"chainId":"0x38","nonce":"0x1","value":"0x0"})
}
#[test]
fn log_mode_keeps_explicit_start_and_confirmation_configuration() {
    for key in [
        "start_block",
        "confirmation_depth",
        "max_backfill_blocks",
        "chunk_blocks",
        "filter",
    ] {
        let mut v = config();
        v.as_object_mut().unwrap().remove(key);
        assert!(
            Config::parse(v).is_err(),
            "confirmed log mode silently defaulted {key}"
        );
    }
}
#[test]
fn pending_matching_uses_pinned_actual_transaction_fields() {
    let c = Config::parse(pending_config("full")).unwrap();
    let e = pending_transaction(1);
    let id = event_id(&c, &e).unwrap();
    assert_eq!(id.len(), 64);
    let mut upper = e.clone();
    upper["hash"] = json!(format!("0x{}", "AB".repeat(32)));
    let mut lower = upper.clone();
    lower["hash"] = json!(format!("0x{}", "ab".repeat(32)));
    assert_eq!(event_id(&c, &upper).unwrap(), event_id(&c, &lower).unwrap());
    for (key, val) in [
        ("from", json!("0x4444444444444444444444444444444444444444")),
        ("to", json!("0x4444444444444444444444444444444444444444")),
        ("to", Value::Null),
        ("input", json!(format!("0xabcdef02{:064x}", 17))),
        ("input", json!(format!("0xabcdef01{:064x}", 18))),
        ("blockHash", json!(format!("0x{}", "aa".repeat(32)))),
        ("blockNumber", json!("0x1")),
        ("transactionIndex", json!("0x0")),
        ("chainId", json!("0x1")),
        ("input", json!("0xabcdef0100")),
        ("hash", json!("0x1")),
    ] {
        let mut bad = e.clone();
        bad[key] = val;
        assert!(
            event_id(&c, &bad).is_err(),
            "triggered on invalid/mismatched {key}"
        );
    }
    for key in ["from", "to", "input", "hash", "blockHash", "blockNumber"] {
        let mut bad = e.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(event_id(&c, &bad).is_err(), "triggered with missing {key}");
    }
    let mut optional = e;
    optional.as_object_mut().unwrap().remove("chainId");
    optional.as_object_mut().unwrap().remove("transactionIndex");
    assert_eq!(event_id(&c, &optional).unwrap(), id);
    assert!(
        event_id(&c, &event()).is_err(),
        "receipt log used as pending input"
    );
}
#[tokio::test]
async fn pending_words_bind_only_real_input_words_or_constants() {
    let mut v = pending_config("full");
    let call = v["action"]["call"].clone();
    for source in [
        json!({"kind":"topic","index":0}),
        json!({"kind":"data","index":0}),
        json!({"kind":"read","to":"0x2222222222222222222222222222222222222222","input_data":"0x12345678","word_index":0}),
    ] {
        v["action"] = json!({"kind":"words","call":call,"args":[{"source":source,"abi_type":"uint256","max":"20"}]});
        assert!(
            Config::parse(v.clone()).is_err(),
            "accepted log-bound source in pending action"
        );
    }
    v["action"] = json!({"kind":"words","call":call,"args":[
        {"source":{"kind":"input","index":0},"abi_type":"uint256","max":"20"},
        {"source":{"kind":"constant","word":format!("0x{:064x}",3)},"abi_type":"uint256","max":"3"}]});
    let c = Config::parse(v.clone()).unwrap();
    assert_eq!(
        c.build(&pending_transaction(1)).await.unwrap().0.input_data,
        format!("0x12345678{:064x}{:064x}", 17, 3)
    );
    v["action"]["args"][0]["source"]["index"] = json!(256);
    assert!(Config::parse(v).is_err());
    let mut log = config();
    log["action"] = json!({"kind":"words","call":call,"args":[{"source":{"kind":"input","index":0},"abi_type":"uint256","max":"20"}]});
    assert!(
        Config::parse(log).is_err(),
        "logs must not invent transaction input"
    );
}
#[test]
fn pending_source_requires_explicit_pins_and_rejects_log_settings() {
    let v = pending_config("hashes");
    assert!(
        Config::parse(v.clone()).is_ok(),
        "pending source unavailable"
    );
    for key in ["from", "to", "selector", "args", "subscription"] {
        let mut bad = v.clone();
        bad["source"].as_object_mut().unwrap().remove(key);
        assert!(Config::parse(bad).is_err(), "accepted missing {key}");
    }
    for (key, value) in [
        ("confirmation_depth", json!(1)),
        ("start_block", json!(1)),
        ("max_backfill_blocks", json!(1)),
        ("chunk_blocks", json!(1)),
        ("filter", config()["filter"].clone()),
    ] {
        let mut bad = v.clone();
        bad[key] = value;
        assert!(
            Config::parse(bad).is_err(),
            "pending accepted log option {key}"
        );
    }
    for (key, value) in [
        ("subscription", json!("auto")),
        ("from", json!("any")),
        ("to", json!(null)),
        ("selector", json!("0xab")),
        (
            "args",
            json!([{"index":256,"abi_type":"uint256","equals":format!("0x{:064x}",17)}]),
        ),
        (
            "args",
            json!([{"index":0,"abi_type":"string","equals":format!("0x{:064x}",17)}]),
        ),
        (
            "args",
            json!([{"index":0,"abi_type":"bool","equals":format!("0x{:064x}",2)}]),
        ),
        (
            "args",
            json!([{"index":0,"abi_type":"address","equals":format!("0x{}","ff".repeat(32))}]),
        ),
    ] {
        let mut bad = v.clone();
        bad["source"][key] = value;
        assert!(Config::parse(bad).is_err(), "accepted invalid {key}");
    }
    assert!(Config::parse(pending_config("full")).is_ok());
    let mut legacy = config();
    legacy["source"] = json!({"kind":"logs"});
    assert!(Config::parse(legacy).is_ok());
}
pub(super) fn event() -> Value {
    json!({"address":"0x2222222222222222222222222222222222222222","topics":[format!("0x{}","11".repeat(32)),format!("0x{:064x}",17)],"data":format!("0x{:064x}",29),"blockNumber":"0x1","blockHash":format!("0x{}","aa".repeat(32)),"transactionHash":format!("0x{}","bb".repeat(32)),"logIndex":"0x0","removed":false})
}
pub(super) fn route_config(key: &Path) -> Value {
    json!({"chain_index":"56","binance_chain_id":"56",
        "from_token_address":"0x3333333333333333333333333333333333333333","to_token_address":"0x4444444444444444444444444444444444444444",
        "amount":"1000","slippage_percent":"1","user_wallet_address":"0x1111111111111111111111111111111111111111",
        "expected_spender":"0x2222222222222222222222222222222222222222",
        "allowed_router_addresses":["0x2222222222222222222222222222222222222222"],
        "allowed_selectors":["0x38ed1739"],"max_value_wei":"0","max_gas_limit":500000,
        "timeout_ms":1000,"connect_timeout_ms":1000,"max_wait_ms":1000,
        "min_request_interval_ms":1,"max_retries":0,"retry_backoff_ms":1,
        "credential_file":key.to_string_lossy()})
}
pub(super) fn synthetic_route_key(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let key = dir.join("okx.json");
    std::fs::write(&key, br#"{"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass"}"#).unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    key
}
#[tokio::test]
async fn route_action_is_resolved_only_after_the_event_and_never_before() {
    let dir = tempfile::tempdir().unwrap();
    let key = synthetic_route_key(dir.path());
    let mut v = config();
    v["action"] = json!({"kind":"route","config":route_config(&key)});
    let mut c = Config::parse(v).unwrap();
    // Load has no route/deadline binding, and cannot build before arm.
    assert_eq!(c.deadline_ms, 0);
    assert!(c.monotonic_deadline.is_none());
    let e = c.build(&event()).await.unwrap_err();
    assert!(e.contains("deadline"), "unexpected route failure: {e}");
    // Simulate the same resolved values captured by first arm, already expired now.
    c.deadline_ms = 1;
    c.monotonic_deadline = Some(std::time::Instant::now());
    let e = c.build(&event()).await.unwrap_err();
    assert!(e.contains("deadline"), "unexpected route failure: {e}");
}
#[test]
fn native_route_rejects_user_absolute_deadline_even_with_relative_duration() {
    let dir = tempfile::tempdir().unwrap();
    let key = synthetic_route_key(dir.path());
    let mut v = config();
    let mut r = route_config(&key);
    for absolute in [json!(1), json!(4102444800000u64), Value::Null] {
        r["deadline_unix_ms"] = absolute;
        v["action"] = json!({"kind":"route","config":r});
        let e = Config::parse(v.clone())
            .err()
            .expect("nested absolute deadline accepted");
        assert!(
            e.contains("deadline_unix_ms") && e.contains("valid_for_ms"),
            "{e}"
        );
    }
}
#[test]
fn native_restore_never_renews_duration() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native-event-state.json");
    let deadline = chrono::Utc::now().timestamp_millis() as u64 + 30000;
    let body = json!({"current":{"config":config(),"resolved_deadline_ms":deadline,"status":"READY","operation_id":null,"detail":{}},"history":{}});
    std::fs::write(&path, body.to_string()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut book = Book::default();
    book.restore(dir.path()).unwrap();
    assert_eq!(book.status()["status"], "DISARMED");
    assert_eq!(book.status()["resolved_deadline_ms"], deadline);
}
pub(super) fn legacy_state(status: &str) -> Value {
    let mut c = config();
    c.as_object_mut().unwrap().remove("valid_for_ms");
    c["deadline_ms"] = json!(123456789u64);
    json!({"current":{"config":c,"status":status,"operation_id":"evt-legacy",
        "detail":{"event":{"transactionHash":"old-evidence"},"result":{"orderId":"old-order"}}},
        "history":{"old-success":"SUCCESS","old-unknown":"UNKNOWN","old-failed":"FAILED"}})
}
#[test]
fn native_legacy_migration_preserves_evidence_and_is_repeatable_for_all_statuses() {
    use std::os::unix::fs::PermissionsExt;
    for original in [
        "LOADED",
        "PREPARING",
        "CONNECTING",
        "READY",
        "ACCEPTED",
        "EXECUTING",
        "SUCCESS",
        "FAILED",
        "UNKNOWN",
        "DISARMED",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native-event-state.json");
        let body = legacy_state(original);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(&path, body.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let expected = match original {
            "ACCEPTED" | "EXECUTING" => "UNKNOWN",
            "LOADED" | "PREPARING" | "CONNECTING" | "READY" => "DISARMED",
            s => s,
        };
        for _ in 0..2 {
            let mut book = Book::default();
            book.restore(dir.path()).unwrap();
            book.restore(dir.path()).unwrap();
            let s = book.status();
            assert_eq!(s["status"], expected, "{original}");
            assert_eq!(s["operation_id"], "evt-legacy");
            assert_eq!(s["resolved_deadline_ms"], 123456789u64);
            assert_eq!(s["valid_for_ms"], 0, "migration invented a duration");
            assert_eq!(s["detail"], body["current"]["detail"]);
            let disk: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            for id in ["old-success", "old-unknown", "old-failed"] {
                assert_eq!(disk["history"][id], body["history"][id]);
            }
            assert_eq!(disk["legacy_records"]["claim-1"], body["current"]);
        }
    }
}
#[test]
fn native_legacy_migration_failure_never_publishes_or_discards_state() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native-event-state.json");
    let original = legacy_state("ACCEPTED").to_string();
    std::fs::write(&path, &original).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    // Read succeeds; persist fails its mandatory private-directory check (also under root).
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut book = Book::default();
    for _ in 0..2 {
        assert!(book.restore(dir.path()).unwrap_err().contains("directory"));
        assert_eq!(book.status()["status"], "EMPTY");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    book.restore(dir.path()).unwrap();
    assert_eq!(book.status()["status"], "UNKNOWN");
    // Expanded migration must not silently trim evidence to fit the reader cap.
    let mut big = legacy_state("ACCEPTED");
    big["current"]["detail"]["large"] = json!("x".repeat(140000));
    let original = big.to_string();
    std::fs::write(&path, &original).unwrap();
    let mut book = Book::default();
    for _ in 0..2 {
        assert!(book.restore(dir.path()).unwrap_err().contains("cap"));
        assert_eq!(book.status()["status"], "EMPTY");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
#[test]
fn restart_labels_unsubmitted_accepted_as_failed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let write = |status: &str| {
        let body = json!({"current":{"config":config(),"status":status,
            "operation_id":"evt-deadbeef","detail":{}},"history":{}});
        let path = dir.path().join("native-event-state.json");
        std::fs::write(&path, body.to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    };
    write("ACCEPTED");
    let mut book = Book::default();
    book.restore(dir.path()).unwrap();
    let current = book.status();
    assert_eq!(
        current["status"],
        json!("FAILED"),
        "an unsubmitted accepted task must not become UNKNOWN"
    );
    assert_eq!(current["detail"]["submission_attempted"], json!(false));
    write("EXECUTING");
    let mut book = Book::default();
    book.restore(dir.path()).unwrap();
    assert_eq!(book.status()["status"], json!("UNKNOWN"));
}
#[test]
fn misaligned_abi_blobs_are_rejected_and_removed_logs_are_not_triggers() {
    // Whole 32-byte words only: a trailing partial word means non-canonical source data.
    assert!(word(&format!("0x{}", "00".repeat(64)), 0).is_ok());
    assert!(
        word(&format!("0x{}", "00".repeat(65)), 0).is_err(),
        "accepted trailing partial word"
    );
    assert!(
        word(&format!("0x{}", "00".repeat(63)), 0).is_err(),
        "accepted a truncated word"
    );
    let mut e = event();
    e["removed"] = json!(true);
    let parsed = Config::parse(config()).unwrap();
    assert!(
        event_id(&parsed, &e).is_err(),
        "removed log must not produce an operation id"
    );
    let mut absent = event();
    absent.as_object_mut().unwrap().remove("removed");
    assert!(
        event_id(&parsed, &absent).is_ok(),
        "nodes that omit removed must still be usable"
    );
    let mut weird = event();
    weird["removed"] = json!("maybe");
    assert!(
        event_id(&parsed, &weird).is_err(),
        "non-boolean removed must fail closed"
    );
}
#[test]
fn failures_before_submission_are_failed_not_unknown() {
    let note = json!({"requires_runtime_preview":false});
    let m = json!({"local":true});
    let (status, detail) = fire_outcome(
        false,
        Err("policy rejected before execute".into()),
        &note,
        m.clone(),
    );
    assert_eq!(status, "FAILED");
    assert_eq!(detail["submission_attempted"], json!(false));
    assert!(
        detail.get("retry").is_none(),
        "pre-submit failure must not ask for reconciliation"
    );
    let (status, detail) =
        fire_outcome(true, Err("execute response lost".into()), &note, m.clone());
    assert_eq!(status, "UNKNOWN");
    assert_eq!(detail["retry"], json!("never; reconcile operation journal"));
    let (status, detail) = fire_outcome(true, Ok(json!({"execution":{"txHash":"0xaa"}})), &note, m);
    assert_eq!(status, "SUCCESS");
    assert_eq!(
        detail["success_scope"],
        json!("backend_acknowledged_not_chain_confirmation")
    );
}
#[test]
fn opaque_route_metadata_requires_explicit_opt_in_and_proof_honesty() {
    let mut v = config();
    v["action"] = json!({"kind":"fixed","call":v["action"]["call"]});
    let strict = Config::parse(v.clone()).unwrap();
    let opaque = json!({"requires_runtime_preview":true,"recipient_verified":false,"calldata_min_out_verified":false});
    assert!(
        strict.route_gate(&opaque).is_err(),
        "opaque route accepted without opt-in"
    );
    let bound = json!({"requires_runtime_preview":false,"recipient_verified":true,"calldata_min_out_verified":true});
    assert!(strict.route_gate(&bound).is_ok());
    assert!(
        strict
            .route_gate(&json!({"requires_runtime_preview":false,"recipient_verified":true}))
            .is_err(),
        "accepted a route claiming full validation without minimum proof"
    );
    assert!(
        strict
            .route_gate(&json!({"recipient_verified":true}))
            .is_err(),
        "accepted route metadata with no preview flag"
    );
    v["allow_unbound_route_calldata"] = json!(true);
    let optin = Config::parse(v).unwrap();
    assert!(
        optin.route_gate(&opaque).is_ok(),
        "opt-in still rejected opaque route"
    );
}
#[tokio::test]
async fn log_dedup_window_bounds_memory_without_ending_a_long_task() {
    // A multi-hour task on a busy contract can observe far more distinct logs than any fixed
    // dedup window. Reaching the window is a memory bound, never a task-ending condition.
    let c = Config::parse(config()).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8192);
    let mut seen = RecentIds::default();
    for n in 0..5000u64 {
        let mut e = event();
        e["logIndex"] = json!(format!("0x{n:x}"));
        enqueue(&c, e, "ws", &tx, &mut seen)
            .unwrap_or_else(|err| panic!("distinct log {n} ended the task: {err}"));
    }
    assert!(
        seen.len() <= 4096,
        "dedup metadata grew without bound: {}",
        seen.len()
    );
    let mut queued = 0;
    while rx.try_recv().is_ok() {
        queued += 1;
    }
    assert_eq!(queued, 5000, "accepted events were not queued once each");
    // A repeat of a recently observed id stays suppressed.
    let mut repeat = event();
    repeat["logIndex"] = json!(format!("0x{:x}", 4999));
    enqueue(&c, repeat, "ws", &tx, &mut seen).unwrap();
    assert!(rx.try_recv().is_err(), "duplicate id was queued again");
    // The next matching event still enters the queue exactly once, so it can fire once.
    let mut next = event();
    next["logIndex"] = json!(format!("0x{:x}", 5000));
    enqueue(&c, next.clone(), "ws", &tx, &mut seen).unwrap();
    enqueue(&c, next, "ws", &tx, &mut seen).unwrap();
    assert!(rx.try_recv().is_ok(), "matching event was not queued");
    assert!(rx.try_recv().is_err(), "matching event was queued twice");
}
#[test]
fn persist_failure_detail_never_relabels_a_submitted_operation() {
    let note = json!({"requires_runtime_preview":false});
    let m = json!({"local":true});
    let (status, detail) = fire_outcome(
        true,
        Ok(json!({"execution":{"txHash":"0xaa"}})),
        &note,
        m.clone(),
    );
    let merged = persist_failure_detail(status, detail, "disk full");
    assert_eq!(merged["submission_attempted"], json!(true));
    assert_eq!(merged["persistence_failed"], json!(true));
    assert_eq!(merged["persistence_error"], json!("disk full"));
    assert_eq!(merged["error"], json!("disk full"));
    assert_eq!(merged["result"]["execution"]["txHash"], "0xaa");
    assert_eq!(merged["route"], note);
    assert_eq!(merged["retry"], json!("never; reconcile operation journal"));
    let (status, detail) = fire_outcome(true, Err("execute response lost".into()), &note, m);
    let merged = persist_failure_detail(status, detail, "disk full");
    assert_eq!(merged["error"], json!("execute response lost"));
    assert_eq!(merged["submission_attempted"], json!(true));
    assert_eq!(merged["retry"], json!("never; reconcile operation journal"));
    // A provably unsent fire stays unsent and gains no reconciliation instruction.
    let (status, detail) = fire_outcome(false, Err("policy rejected".into()), &note, json!({}));
    let merged = persist_failure_detail(status, detail, "disk full");
    assert_eq!(merged["submission_attempted"], json!(false));
    assert!(merged.get("retry").is_none(), "{merged}");
    assert_eq!(merged["error"], json!("policy rejected"));
}
#[test]
fn failed_state_write_never_leaves_a_temp_orphan() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let temp = dir
        .path()
        .join(format!(".native-event-{:x}", 0xdeadbeefu64));
    let orphans = || {
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".native-event-"))
            .collect::<Vec<_>>()
    };
    // A write/sync fault after O_CREAT must not leave the 0600 temp file behind. Repeated faults
    // are simulated too, because accumulating orphans is the reported failure mode.
    for n in 0..3u64 {
        let temp = dir
            .path()
            .join(format!(".native-event-{:x}", 0xdeadbeef + n));
        let error = write_temp_with(&temp, b"{\"current\":null}", |_, _| {
            Err(std::io::Error::from_raw_os_error(libc::EIO))
        })
        .unwrap_err();
        assert!(error.contains("sync"), "unexpected error: {error}");
        assert!(
            !temp.exists(),
            "failed write {n} left its temp state file behind"
        );
        assert!(
            orphans().is_empty(),
            "failed state writes left temp files: {:?}",
            orphans()
        );
    }
    // A create failure owns nothing and must not remove the pre-existing file.
    std::fs::write(&temp, b"pre-existing").unwrap();
    assert!(write_temp_with(&temp, b"x", |_, _| Ok(()))
        .unwrap_err()
        .contains("create"));
    assert_eq!(std::fs::read(&temp).unwrap(), b"pre-existing");
}
#[tokio::test]
async fn native_fixed_and_word_binding_are_generic_calls() {
    let c = Config::parse(config()).unwrap();
    assert_eq!(c.build(&event()).await.unwrap().0.input_data, "0x12345678");
    let mut v = config();
    v["action"] = json!({"kind":"words","call":v["action"]["call"],"args":[{"source":{"kind":"topic","index":1},"abi_type":"uint256","max":"20"},{"source":{"kind":"data","index":0},"abi_type":"uint256","max":"30"}]});
    let c = Config::parse(v).unwrap();
    assert_eq!(
        c.build(&event()).await.unwrap().0.input_data,
        format!("0x12345678{:064x}{:064x}", 17, 29)
    );
    let mut bad = event();
    bad["topics"][1] = json!(format!("0x{:064x}", 21));
    assert!(c.build(&bad).await.is_err());
}
