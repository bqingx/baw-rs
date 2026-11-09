use super::*;
use crate::{client::Client, signing::check_wallet_policy};
use std::sync::{atomic::AtomicBool, Arc};
const SENDER: &str = "0x1111111111111111111111111111111111111111";
fn client(id: &str) -> Client {
    Client::test_client(
        crate::session::Credentials {
            client_id: zeroize::Zeroizing::new(id.into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        },
        "http://127.0.0.1:9",
    )
    .unwrap()
}
async fn bound<F: std::future::Future>(
    policy: PreparedWalletPolicy,
    deadline: u64,
    cancel: bool,
    future: F,
) -> F::Output {
    binding_scope(
        Arc::new(|| Ok(())),
        task_scope(
            deadline,
            Arc::new(AtomicBool::new(cancel)),
            prepared_wallet_scope(policy, future),
        ),
    )
    .await
}
#[tokio::test]
async fn pinned_policy_works_without_backend_reads_even_if_skip_is_requested() {
    let mut c = client("synthetic-one");
    let policy = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    let result = bound(
        policy,
        u64::MAX,
        false,
        check_wallet_policy(&mut c, "56", Some(SENDER), true),
    )
    .await
    .unwrap();
    assert_eq!(result, (SENDER.into(), 4102444800));
    assert!(
        prepared_wallet().is_none(),
        "policy scope leaked into the next command"
    );
}
#[tokio::test]
async fn prepared_execution_requires_both_deadline_and_identity_gates() {
    let mut c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    assert!(prepared_wallet_scope(
        p.clone(),
        check_wallet_policy(&mut c, "56", Some(SENDER), false)
    )
    .await
    .is_err());
    assert!(task_scope(
        u64::MAX,
        Arc::new(AtomicBool::new(false)),
        prepared_wallet_scope(p, check_wallet_policy(&mut c, "56", Some(SENDER), false))
    )
    .await
    .is_err());
}
#[tokio::test]
async fn expired_or_cancelled_task_cannot_use_the_pinned_policy() {
    let mut c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    for (deadline, cancel) in [(1, false), (u64::MAX, true)] {
        assert!(bound(
            p.clone(),
            deadline,
            cancel,
            check_wallet_policy(&mut c, "56", Some(SENDER), false)
        )
        .await
        .is_err());
    }
}
#[tokio::test]
async fn changed_wallet_binding_blocks_cached_policy_before_any_request() {
    let mut c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    let result = binding_scope(
        Arc::new(|| Err("changed identity".into())),
        task_scope(
            u64::MAX,
            Arc::new(AtomicBool::new(false)),
            prepared_wallet_scope(p, check_wallet_policy(&mut c, "56", Some(SENDER), false)),
        ),
    )
    .await;
    assert!(result.is_err());
}
#[tokio::test]
async fn expired_mode_different_sender_chain_and_client_do_not_fall_back_to_network() {
    let mut c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    assert!(bound(
        p.clone(),
        u64::MAX,
        false,
        check_wallet_policy(&mut c, "1", Some(SENDER), false)
    )
    .await
    .is_err());
    assert!(bound(
        p.clone(),
        u64::MAX,
        false,
        check_wallet_policy(
            &mut c,
            "56",
            Some("0x2222222222222222222222222222222222222222"),
            false
        )
    )
    .await
    .is_err());
    let mut other = client("synthetic-two");
    assert!(bound(
        p.clone(),
        u64::MAX,
        false,
        check_wallet_policy(&mut other, "56", Some(SENDER), false)
    )
    .await
    .is_err());
    let mut expired = p.clone();
    expired.developer_mode_expires_at = 1;
    assert!(bound(
        expired,
        u64::MAX,
        false,
        check_wallet_policy(&mut c, "56", Some(SENDER), false)
    )
    .await
    .is_err());
    c.invalidate();
    assert!(bound(
        p,
        u64::MAX,
        false,
        check_wallet_policy(&mut c, "56", Some(SENDER), false)
    )
    .await
    .is_err());
}
#[tokio::test]
async fn prepared_policy_is_not_a_global_switch_or_inherited_by_other_tasks() {
    let c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    bound(p, u64::MAX, false, async {
        assert!(check_auxiliary_read().is_err());
        assert!(tokio::spawn(async { check_auxiliary_read().is_ok() })
            .await
            .unwrap());
    })
    .await;
    assert!(check_auxiliary_read().is_ok());
}
#[tokio::test]
async fn prepared_http_firewall_accepts_only_the_two_exact_sdk_endpoints() {
    let c = client("synthetic-one");
    let p = PreparedWalletPolicy::capture(&c, "56", SENDER, 4102444800).unwrap();
    bound(p, u64::MAX, false, async {
        for method in [reqwest::Method::GET, reqwest::Method::POST] {
            for suffix in [
                "/settings/query",
                "/mpc-wallet/list",
                "/trading-limit/query",
                "/order/tx-lock-status",
                "/login/query",
            ] {
                assert!(
                    check_prepared_http(&method, &format!("{}{suffix}", crate::client::API))
                        .is_err()
                );
            }
        }
        for suffix in ["/contract-call/preview", "/contract-call/execute"] {
            let path = format!("{}{suffix}", crate::client::API);
            assert!(check_prepared_http(&reqwest::Method::POST, &path).is_ok());
            assert!(check_prepared_http(&reqwest::Method::GET, &path).is_err());
            assert!(
                check_prepared_http(&reqwest::Method::POST, &format!("{path}?other=1")).is_err()
            );
        }
    })
    .await;
}
