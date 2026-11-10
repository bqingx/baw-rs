use super::*;
use crate::trade::Provider;

fn args() -> QuoteArgs {
    QuoteArgs {
        provider: Provider::Okx,
        okx_config: Some("/tmp/fake-okx-tax-profile.json".into()),
        from_token_decimals: Some(18),
        binance_chain_id: "56".into(),
        from_token: "0x1234567890123456789012345678901234567890".into(),
        to_token: "0x9876543210987654321098765432109876543210".into(),
        from_token_qty: "1".into(),
        slippage: "1".into(),
        max_from_tax_percent: "0".into(),
        max_to_tax_percent: "0".into(),
    }
}

fn row(from: &str, to: &str) -> Value {
    json!({"fromToken": {"taxRate": from}, "toToken": {"taxRate": to}})
}

#[test]
fn zero_metadata_needs_no_opt_in() {
    let q = args();
    validate_caps(&q).unwrap();
    assert!(!Taxes::from_quote(&q, &row("0", "0.000")).unwrap().any());
}

#[test]
fn input_output_and_both_taxes_require_independent_opt_ins() {
    for (from, to) in [("0.02", "0"), ("0", "0.02"), ("0.02", "0.02")] {
        let mut q = args();
        let metadata = row(from, to);
        let err = Taxes::from_quote(&q, &metadata).err().unwrap();
        assert!(err.contains(if from != "0" {
            "--max-from-tax-percent"
        } else {
            "--max-to-tax-percent"
        }));
        q.max_from_tax_percent = if from == "0" { "0" } else { "2" }.into();
        q.max_to_tax_percent = if to == "0" { "0" } else { "2" }.into();
        assert!(Taxes::from_quote(&q, &metadata).unwrap().any());
    }
    let mut q = args();
    q.max_from_tax_percent = "99".into();
    assert!(Taxes::from_quote(&q, &row("0", "0.02"))
        .err()
        .unwrap()
        .contains("--max-to-tax-percent"));
    q.max_from_tax_percent = "0".into();
    q.max_to_tax_percent = "99".into();
    assert!(Taxes::from_quote(&q, &row("0.02", "0"))
        .err()
        .unwrap()
        .contains("--max-from-tax-percent"));
}

#[test]
fn exact_equality_and_one_rate_unit_over_cap() {
    for side in ["fromToken", "toToken"] {
        let mut q = args();
        q.max_from_tax_percent = "2.00000001".into();
        q.max_to_tax_percent = "2.00000001".into();
        let mut metadata = row("0", "0");
        metadata[side]["taxRate"] = json!("0.020000000100000000");
        assert!(Taxes::from_quote(&q, &metadata).is_ok());
        metadata[side]["taxRate"] = json!("0.020000000100000001");
        assert!(Taxes::from_quote(&q, &metadata).is_err());
    }
}

#[test]
fn rejects_unknown_malformed_and_out_of_range_rates_on_either_side() {
    let mut q = args();
    q.max_from_tax_percent = "99.99999999".into();
    q.max_to_tax_percent = "99.99999999".into();
    for side in ["fromToken", "toToken"] {
        for invalid in [
            Value::Null,
            json!(0.02),
            json!(false),
            json!({}),
            json!("unknown"),
            json!(""),
            json!("-0.02"),
            json!("+0.02"),
            json!("2e-2"),
            json!(".02"),
            json!("0."),
            json!(" 0.02"),
            json!("0.0200000000000000000"),
            json!("1"),
            json!("1.01"),
            json!("100"),
        ] {
            let mut metadata = row("0", "0");
            metadata[side]["taxRate"] = invalid.clone();
            assert!(
                Taxes::from_quote(&q, &metadata).is_err(),
                "{side}: {invalid}"
            );
        }
        let mut metadata = row("0", "0");
        metadata[side].as_object_mut().unwrap().remove("taxRate");
        assert!(Taxes::from_quote(&q, &metadata).is_err());
        metadata.as_object_mut().unwrap().remove(side);
        assert!(Taxes::from_quote(&q, &metadata).is_err());
    }
}

#[test]
fn caps_are_independently_validated_even_with_zero_tax() {
    for invalid in [
        "",
        "-1",
        "+1",
        "1e0",
        "auto",
        "100",
        "101",
        "0.000000001",
        "2.",
        " 2",
    ] {
        for from in [true, false] {
            let mut q = args();
            let flag = if from {
                q.max_from_tax_percent = invalid.into();
                "--max-from-tax-percent"
            } else {
                q.max_to_tax_percent = invalid.into();
                "--max-to-tax-percent"
            };
            assert!(validate_caps(&q).unwrap_err().contains(flag));
            assert!(Taxes::from_quote(&q, &row("0", "0")).is_err());
        }
    }
    let mut q = args();
    q.max_from_tax_percent = "0.00000001".into();
    q.max_to_tax_percent = "99.99999999".into();
    validate_caps(&q).unwrap();
}

#[test]
fn native_tax_contradictions_override_opt_in() {
    for native in [
        "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "0xEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
        "0x0000000000000000000000000000000000000000",
    ] {
        for from in [true, false] {
            let mut q = args();
            q.max_from_tax_percent = "2".into();
            q.max_to_tax_percent = "2".into();
            if from {
                q.from_token = native.into();
            } else {
                q.to_token = native.into();
            }
            assert!(Taxes::from_quote(&q, &row("0", "0")).is_ok());
            let metadata = if from {
                row("0.02", "0")
            } else {
                row("0", "0.02")
            };
            assert!(Taxes::from_quote(&q, &metadata)
                .err()
                .unwrap()
                .contains("native"));
        }
    }
}

#[test]
fn arbitrary_tokens_and_exact_metadata_summary() {
    let mut q = args();
    q.max_from_tax_percent = "2".into();
    q.max_to_tax_percent = "0.00000001".into();
    let taxes = Taxes::from_quote(&q, &row("0.0200", "0.000000000000000001")).unwrap();
    assert_eq!(taxes.from_rate, BigUint::from(20_000_000_000_000_000u64));
    assert_eq!(taxes.to_rate, BigUint::from(1u32));
    let summary = taxes.summary(&q);
    assert_eq!(summary["fromToken"]["taxRate"], "0.0200");
    assert_eq!(summary["fromToken"]["taxPercent"], "2");
    assert_eq!(summary["toToken"]["taxPercent"], "0.0000000000000001");
    assert_eq!(summary["fromToken"]["maxTaxPercent"], "2");
    assert_eq!(summary["toToken"]["maxTaxPercent"], "0.00000001");
    let warning = summary["warning"].as_str().unwrap();
    assert!(warning.contains("not guarantees"));
    assert!(warning.contains("Rates can change"));
}
