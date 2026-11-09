use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn word(s: &str) -> String {
    format!("{:0>64}", s)
}
fn calldata(minimum: u64) -> String {
    format!(
        "0x38ed1739{}{}{}{}{}{}{}{}",
        word(&format!("{:x}", 10000000000000000001u64)),
        word(&format!("{minimum:x}")),
        word("a0"),
        word(&WALLET[2..]),
        word("ffffffffffffffff"),
        word("2"),
        word(&INPUT[2..]),
        word(&OUTPUT[2..])
    )
}
fn response(minimum: u64) -> Value {
    json!({"code":"0","data":[{"routerResult":{"chainIndex":"56","fromTokenAmount":"10000000000000000001",
      "toTokenAmount":minimum.saturating_add(1).to_string(),"fromToken":{"tokenContractAddress":INPUT},"toToken":{"tokenContractAddress":OUTPUT}},
      "tx":{"from":WALLET,"to":ROUTER,"value":"0","gas":"200000","gasPrice":"1000000000",
      "minReceiveAmount":minimum.to_string(),"data":calldata(minimum)}}]})
}
fn validate(c: &Value, r: &Value) -> Result<Value, String> {
    RouteActionBuilder::validate_only(c, r)
}

fn assert_opaque_validation(call: &Value) {
    let validation = &call["route_validation"];
    assert_eq!(validation["recipient_verified"], false);
    assert_eq!(validation["calldata_min_out_verified"], false);
    assert_eq!(validation["requires_runtime_preview"], true);
    assert_eq!(validation["minimum_output_semantics"], "unknown");
    assert_eq!(validation["calldata_policy"], "pinned_okx_opaque_v3");
}

#[test]
fn arbitrary_and_old_known_selectors_are_preserved_without_local_verification() {
    let c = fixture();
    for data in [
        "0x01".to_string(),
        "0x123456780000".to_string(),
        "0xf2c42696".to_string(),
        "0x9871efa4".to_string(),
        "0xABCDEF01aB".to_string(),
        calldata(99),
        calldata(99).replacen("0x38ed1739", "0x5c11d795", 1),
    ] {
        let mut r = response(99);
        r["data"][0]["tx"]["data"] = json!(data);
        let call = validate(&c, &r).unwrap();
        assert_eq!(call["input_data"], data);
        assert_opaque_validation(&call);
    }
}

#[test]
fn old_v2_layout_amount_path_recipient_deadline_and_minimum_are_opaque() {
    let c = fixture();
    for (idx, replacement) in [
        (0, word("1")),
        (1, word("1")),
        (2, word("80")),
        (3, word(&ROUTER[2..])),
        (3, format!("1{}", &word(&WALLET[2..])[1..])),
        (4, word("1")),
        (5, word("1")),
        (6, word(&OUTPUT[2..])),
        (7, word(&INPUT[2..])),
    ] {
        let mut r = response(99);
        let mut data = calldata(99);
        data.replace_range(10 + idx * 64..10 + (idx + 1) * 64, &replacement);
        for selector in ["0x38ed1739", "0x5c11d795"] {
            let altered = data.replacen("0x38ed1739", selector, 1);
            r["data"][0]["tx"]["data"] = json!(altered);
            let call = validate(&c, &r).unwrap();
            assert_eq!(call["input_data"], altered, "changed word {idx}");
            assert_opaque_validation(&call);
        }
    }
    for selector in ["0x38ed1739", "0x5c11d795"] {
        for data in [
            selector.to_string(),
            format!("{selector}00"),
            format!("{selector}{}", word("ffffffff")),
            calldata(99).replacen("0x38ed1739", selector, 1)[..74].to_string(),
            format!("{}00", calldata(99).replacen("0x38ed1739", selector, 1)),
        ] {
            let mut r = response(99);
            r["data"][0]["tx"]["data"] = json!(data);
            let call = validate(&c, &r).unwrap();
            assert_eq!(call["input_data"], data);
            assert_opaque_validation(&call);
        }
    }
}

#[test]
fn reported_minimum_is_required_independently_of_opaque_calldata() {
    let c = fixture();
    let mut r = response(99);
    r["data"][0]["tx"]
        .as_object_mut()
        .unwrap()
        .remove("minReceiveAmount");
    assert!(validate(&c, &r).is_err(), "quote amount is not a minimum");
    assert!(validate(&c, &response(0)).is_err());
    for minimum in ["1", "98", "100"] {
        let mut r = response(99);
        r["data"][0]["tx"]["minReceiveAmount"] = json!(minimum);
        let call = validate(&c, &r).unwrap();
        assert_eq!(call["input_data"], calldata(99));
        assert_eq!(call["route_validation"]["reported_minimum_output"], minimum);
        assert_opaque_validation(&call);
    }
}

#[test]
fn opaque_payloads_still_require_bounded_nonempty_canonical_hex() {
    let c = fixture();
    for invalid in [
        json!("0x"),
        json!("0x0"),
        json!("0xzz"),
        json!("0X01"),
        Value::Null,
        json!(format!("0x{}", "00".repeat(131073))),
    ] {
        let mut r = response(99);
        r["data"][0]["tx"]["data"] = invalid;
        assert!(validate(&c, &r).is_err());
    }
}

#[test]
fn response_echoes_and_gas_units_are_fail_closed() {
    let c = fixture();
    let cases = [
        ("/data/0/routerResult/chainIndex", json!("1")),
        ("/data/0/routerResult/fromTokenAmount", json!("1")),
        ("/data/0/tx/chainId", json!("1")),
        ("/data/0/tx/from", json!(ROUTER)),
        ("/data/0/tx/to", json!(WALLET)),
        ("/data/0/tx/gas", json!("200000.1")),
        ("/data/0/tx/gas", json!("05000")),
        ("/data/0/tx/gas", json!("2000001")),
        ("/data/0/tx/gas", json!(0)),
        ("/data/0/tx/gasLimit", json!("200001")),
        ("/data/0/tx/value", json!("1")),
        ("/data/0/tx/recipient", json!(ROUTER)),
        ("/data/0/routerResult/priceImpactPercent", json!("5")),
        ("/data/0/tx/minReceiveAmount", json!(null)),
    ];
    for (pointer, value) in cases {
        let mut r = response(99);
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        r.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(validate(&c, &r).is_err(), "accepted {pointer}");
    }
}

#[test]
fn opaque_allowlisted_calldata_requires_preview_and_never_claims_verified_recipient_or_minimum() {
    let c = fixture();
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!("0x123456780000");
    r["data"][0]["tx"]["recipient"] = json!(WALLET);
    let call = validate(&c, &r).unwrap();
    assert_eq!(call["route_validation"]["requires_runtime_preview"], true);
    assert_eq!(call["route_validation"]["recipient_verified"], false);
    assert_eq!(call["route_validation"]["calldata_min_out_verified"], false);
    assert!(!call["route_validation"]["limitations"]
        .as_array()
        .unwrap()
        .is_empty());
    r["data"][0]["tx"]
        .as_object_mut()
        .unwrap()
        .remove("minReceiveAmount");
    assert!(validate(&c, &r).is_err());
}

#[test]
fn malformed_or_ambiguous_business_envelopes_are_refused_without_echoing_provider_text() {
    let c = fixture();
    for body in [
        json!({"code":"51000","msg":"synthetic-secret","data":[]}),
        json!({"code":0,"data":[]}),
        json!({"code":"0","data":[]}),
        json!({"code":"0","data":[{},{}]}),
        json!({"code":"0","data":[{"tx":null}]}),
        json!({"code":"0","data":[{"tx":[]}]}),
        json!({"data":[]}),
    ] {
        let e = validate(&c, &body).err().unwrap();
        assert!(!e.contains("synthetic"), "provider text leaked: {e}");
    }
}

#[test]
fn additional_signing_requirements_and_slippage_echo_mismatch_are_refused() {
    let c = fixture();
    let mut r = response(99);
    r["data"][0]["tx"]["signatureData"] = json!(["0x1234"]);
    assert!(
        validate(&c, &r).is_err(),
        "additional signing requirements lost"
    );
    // An empty placeholder means "nothing extra to sign" and stays valid.
    let mut r = response(99);
    r["data"][0]["tx"]["signatureData"] = json!([""]);
    assert!(validate(&c, &r).is_ok());
    let mut r = response(99);
    r["data"][0]["tx"]["slippagePercent"] = json!("50");
    assert!(validate(&c, &r).is_err(), "slippage echo mismatch accepted");
}

#[test]
fn opaque_router_is_pinned_independently_of_the_configured_approval_spender() {
    // Aggregator entry router and allowance target may differ. Neither is inferred from ABI.
    // The signing path separately validates bounded allowance consumption for the pinned spender.
    let c = fixture();
    let mut r = response(99);
    r["data"][0]["approveContract"] = json!(WALLET);
    assert!(
        validate(&c, &r).is_ok(),
        "undocumented approval echo must not fail the swap"
    );
    let mut wrong_spender = c;
    wrong_spender["expected_spender"] = json!(INPUT);
    assert!(
        validate(&wrong_spender, &response(99)).is_ok(),
        "do not infer that router and approval spender must be equal"
    );
}

#[test]
fn zero_minimum_is_refused_but_the_reported_minimum_is_not_re_derived_locally() {
    // `minReceiveAmount` is documented as the minimum received at the slippage limit: the provider's
    // number inside the calldata. Recomputing a floor from the quote only second-guesses its slippage
    // arithmetic, so only "missing/zero" is refused here.
    let c = fixture();
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!("0x12345678");
    r["data"][0]["tx"]["minReceiveAmount"] = json!("0");
    assert!(validate(&c, &r).is_err(), "zero minimum accepted");
    let mut r = response(99);
    r["data"][0]["tx"]["data"] = json!("0x12345678");
    r["data"][0]["routerResult"]["toTokenAmount"] = json!("10000");
    r["data"][0]["tx"]["minReceiveAmount"] = json!("1");
    assert!(
        validate(&c, &r).is_ok(),
        "a reported minimum is no longer re-derived from the quote"
    );
}

#[test]
fn undocumented_approval_metadata_is_ignored_instead_of_failing_the_swap() {
    // Neither `approveTransaction` nor its encoded form appears in the documented swap schema.
    // Validating it only produced failures, so any value is ignored; the real signal for "this swap
    // expects an approval" is a non-empty `signatureData`, which is still refused.
    let c = fixture();
    for value in [
        json!(json!({"approveContract": WALLET, "approveTxCalldata": "0x"}).to_string()),
        json!("malformed synthetic-secret"),
    ] {
        let mut r = response(99);
        r["data"][0]["approveTransaction"] = value;
        assert!(validate(&c, &r).is_ok(), "approval metadata rejected");
    }
}

#[test]
fn hmac_rfc4231_vectors_include_long_keys() {
    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }
    assert_eq!(
        hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hex(&hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const ROUTER: &str = "0x2222222222222222222222222222222222222222";
const INPUT: &str = "0x3333333333333333333333333333333333333333";
const OUTPUT: &str = "0x4444444444444444444444444444444444444444";
fn fixture() -> Value {
    json!({"chain_index":"56","binance_chain_id":"56","from_token_address":INPUT,
      "to_token_address":OUTPUT,"amount":"10000000000000000001","slippage_percent":"0.5",
      "user_wallet_address":WALLET,"expected_spender":ROUTER,"allowed_router_addresses":[ROUTER],
      "max_value_wei":"0","max_gas_limit":2000000})
}
#[test]
fn removed_compatibility_fields_are_unknown_not_ignored() {
    let c = fixture();
    for (key, value) in [
        ("allowed_selectors", json!([])),
        ("require_runtime_preview", json!(false)),
        ("auto_approve", json!(true)),
        ("prepare_at_load", json!(true)),
    ] {
        let mut old = c.clone();
        old[key] = value;
        assert!(
            validate(&old, &response(99)).is_err(),
            "still accepted {key}"
        );
    }
}

#[test]
fn strict_config_accepts_exact_integer_rejects_unknown_or_unsafe_fields() {
    let c = fixture();
    assert!(validate(&c, &response(99)).is_ok());
    for (key, value) in [
        ("unknown", json!(1)),
        ("amount", json!(1.5)),
        ("amount", json!("1e18")),
        ("amount", json!("0")),
        ("slippage_percent", json!("100")),
        ("chain_index", json!("501")),
        ("expected_spender", json!("not-an-address")),
        ("allowed_router_addresses", json!([])),
        ("gas_buffer_percent", json!(0)),
    ] {
        let mut bad = c.clone();
        bad[key] = value;
        assert!(validate(&bad, &response(99)).is_err(), "accepted {key}");
    }
}

#[test]
fn gas_buffer_widens_the_cap_without_exceeding_the_configured_ceiling() {
    // The provider's point estimate is what gets buffered; a 130% / +80k headroom keeps a
    // quote that sits within 0.2% of the requirement from failing the backend simulation.
    assert_eq!(buffered_gas_limit(300_474, 130, 80_000, 1_200_000), 390_616);
    assert_eq!(
        buffered_gas_limit(1_013_123, 130, 80_000, 1_200_000),
        1_200_000
    );
    assert_eq!(buffered_gas_limit(250_000, 100, 0, 1_200_000), 250_000);
    // A cap below the quote never happens (the builder rejects gas > cap first), but the clamp must
    // still bind so a future caller cannot exceed the ceiling.
    assert_eq!(buffered_gas_limit(400_000, 130, 80_000, 300_000), 300_000);
    assert_eq!(buffered_gas_limit(0, 130, 80_000, 1_200_000), 0);
    assert_eq!(buffered_gas_limit(400_000, 130, 80_000, 0), 0);
    assert_eq!(
        buffered_gas_limit(u64::MAX, 500, u64::MAX, u64::MAX),
        u64::MAX
    );
}

#[test]
fn credential_file_is_regular_owner_only_and_errors_do_not_echo_secrets() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new_in(std::env::temp_dir()).unwrap();
    f.write_all(br#"{"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass"}"#).unwrap();
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    let path = f.path().to_str().unwrap().to_owned();
    assert!(load_credentials(&path).is_ok());
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load_credentials(&path).is_err());
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(f.path(),br#"{"api_key":"synthetic-key","secret_key":"synthetic-secret","passphrase":"synthetic-pass","unexpected":true}"#).unwrap();
    let Err(e) = load_credentials(&path) else {
        panic!("credential schema with unknown fields accepted");
    };
    assert!(!e.contains("synthetic"));
    let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let link = dir.path().join("credentials");
    std::os::unix::fs::symlink(f.path(), &link).unwrap();
    assert!(load_credentials(link.to_str().unwrap()).is_err());
}

#[test]
fn price_impact_threshold_is_compared_exactly_and_missing_values_are_tolerated() {
    let mut c = fixture();
    c["max_price_impact_percent"] = json!("1.5");
    // Within threshold (1.49 < 1.5) passes; the field is advisory and may be absent.
    let mut ok = response(99);
    ok.pointer_mut("/data/0/routerResult").unwrap()["priceImpactPercent"] = json!("1.49");
    assert!(validate(&c, &ok).is_ok());
    ok.pointer_mut("/data/0/routerResult").unwrap()["priceImpactPercent"] = json!("1.5");
    assert!(validate(&c, &ok).is_ok(), "equal is not above");
    // Above threshold is refused before anything is signed.
    // OKX returns a negative impact when the trade is favourable (measured: 1 U → BNB = "-0.08").
    ok.pointer_mut("/data/0/routerResult").unwrap()["priceImpactPercent"] = json!("-0.08");
    assert!(validate(&c, &ok).is_ok(), "favourable impact refused");
    ok.pointer_mut("/data/0/routerResult").unwrap()["priceImpactPercent"] = json!("-99");
    assert!(validate(&c, &ok).is_ok(), "large negative impact refused");
    ok.pointer_mut("/data/0/routerResult").unwrap()["priceImpactPercent"] = json!("1.51");
    let err = validate(&c, &ok).unwrap_err();
    assert!(err.contains("price impact"), "{err}");
    ok.pointer_mut("/data/0/routerResult")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("priceImpactPercent");
    assert!(validate(&c, &ok).is_ok());
    // Exact decimal comparison, no floating point.
    assert_eq!(decimal_greater("0.06", "2"), Some(false));
    assert_eq!(decimal_greater("2.0", "2"), Some(false));
    assert_eq!(decimal_greater("2.0001", "2"), Some(true));
    assert_eq!(decimal_greater("-1", "2"), None);
    assert_eq!(decimal_greater("1e3", "2"), None);
    // A malformed threshold is rejected at load.
    let mut bad = c.clone();
    bad["max_price_impact_percent"] = json!("abc");
    assert!(validate(&bad, &response(99)).is_err());
    let mut too_big = c;
    too_big["max_price_impact_percent"] = json!("101");
    assert!(validate(&too_big, &response(99)).is_err());
}
