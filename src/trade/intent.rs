//! Pure intent description shared by the CLI and parent task descriptions.
use super::SwapArgs;
use crate::trade::okx::{amount_units, percentage};
use serde_json::{json, Value};

/// No I/O, quote, simulation, digest, deadline, or execution capability is created.
pub(crate) fn intent_summary(a: &SwapArgs) -> Result<Value, String> {
    let q = &a.quote;
    if !matches!(q.binance_chain_id.as_str(), "1" | "56" | "8453" | "4663") {
        return Err("OKX intent supports the existing EVM chain set only".into());
    }
    super::address(&q.binance_chain_id, &q.from_token)?;
    super::address(&q.binance_chain_id, &q.to_token)?;
    if q.from_token.eq_ignore_ascii_case(&q.to_token) {
        return Err("identical swap tokens".into());
    }
    super::decimal(&q.from_token_qty).and_then(|n| {
        if n <= 0 {
            Err("input quantity must be positive".into())
        } else {
            Ok(n)
        }
    })?;
    let units = q
        .from_token_decimals
        .map(|d| amount_units(&q.from_token_qty, d, true).map(|n| n.to_string()))
        .transpose()?;
    percentage(&q.slippage)?;
    percentage(&q.max_from_tax_percent)?;
    percentage(&q.max_to_tax_percent)?;
    if !matches!(a.mev.as_str(), "true" | "false") {
        return Err("mev must be true or false".into());
    }
    super::gas(&a.gas_level)?;
    if let Some(floor) = &a.okx_min_output_units {
        if floor.is_empty() || !floor.bytes().all(|b| b.is_ascii_digit()) {
            return Err("minimum output floor must be decimal uint256 units".into());
        }
        let n = crate::util::uint::uint256(floor)?;
        if n.bits() == 0 {
            return Err("minimum output floor must be positive".into());
        }
    }
    let mut missing = Vec::new();
    if q.from_token_decimals.is_none() {
        missing.push("fromTokenDecimals");
    }
    if !a.acknowledge_token_risk {
        missing.push("acknowledgeTokenRisk");
    }
    if a.mev != "false" {
        missing.push("mevFalseRequiredByExecutionInterface");
    }
    if !a.gas_level.eq_ignore_ascii_case("MEDIUM") {
        missing.push("mediumGasRequiredByExecutionInterface");
    }
    if (q.from_token == "0x0000000000000000000000000000000000000000"
        || q.from_token
            .eq_ignore_ascii_case("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"))
        && q.from_token_decimals.is_some_and(|d| d != 18)
    {
        missing.push("nativeInputRequires18Decimals");
    }
    Ok(json!({
        "kind":"market-order.intent", "provider":"okx", "localOnly":true,
        "chain":q.binance_chain_id,
        "input":{"token":q.from_token,"quantity":q.from_token_qty,
            "decimals":q.from_token_decimals,"units":units,
            "unitsPolicy":"exact only with explicit decimals; never inferred"},
        "output":{"token":q.to_token,"quantity":null,"decimals":null,
            "userMinimumUnits":a.okx_min_output_units,
            "floorMeaning":"Optional extra fixed net-wallet floor in output base units; absence still requires fresh OKX/slippage minimum and simulation, never zero/1 wei; not an on-chain guarantee"},
        "minimumOutputPolicy":{"mode":if a.okx_min_output_units.is_some(){"fresh_route_plus_fixed_floor"}else{"fresh_route"},"fixedUserMinimumUnits":a.okx_min_output_units,"requiresFreshReportedPositiveMinimum":true,"requiresSlippageBound":true,"requiresSimulatedNetMinimum":true,"quoteBoundAtDescription":false},
        "slippagePercent":q.slippage,
        "taxCapsPercent":{"input":q.max_from_tax_percent,"output":q.max_to_tax_percent},
        "taxWarning":"Metadata caps only; actual tax and execution are not guaranteed. No automatic increases.",
        "approval":{"autoApprove":a.auto_approve,"amountPolicy":"exact input units, never unlimited",
            "when":"only if allowance is insufficient; native input needs no approval",
            "withoutAutoApprove":"insufficient allowance blocks execution",
            "allowanceChecked":false},
        "mev":{"requested":a.mev,"protectedSubmissionSupported":false,
            "executionRequires":"false; no MEV protection promised"},
        "gas":{"requested":a.gas_level,"executionRequires":"MEDIUM",
            "limitation":"Signing interface cannot promise gas-price selection"},
        "riskAccepted":a.acknowledge_token_risk,"riskAcceptanceIsNotAudit":true,
        "readiness":{"missing":missing,"scope":"local fields only; no wallet, funds, allowance, config, quote or route checks"},
        "executionAuthorized":false,"noDeadlineGranted":true,
        "authorizationWarning":"Summary is not authorization or an accepted preview digest. Real swap gates remain required."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{Cli, Command},
        trade::MarketCommand,
    };
    use clap::Parser;

    fn args(extra: &[&str]) -> SwapArgs {
        let mut argv = vec![
            "baw-rs",
            "market-order",
            "intent",
            "--binanceChainId",
            "56",
            "--fromToken",
            "0x1111111111111111111111111111111111111111",
            "--toToken",
            "0x2222222222222222222222222222222222222222",
            "--fromTokenQty",
            "9007199254740993.123456789",
            "--slippage",
            "0.5",
            "--mev",
            "false",
        ];
        argv.extend_from_slice(extra);
        match Cli::try_parse_from(argv).unwrap().command {
            Command::MarketOrder {
                command: MarketCommand::Intent(a),
            } => a,
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn missing_readiness_is_described_without_config_or_authority() {
        let a = args(&[]);
        let v = intent_summary(&a).unwrap();
        assert!(v["input"]["units"].is_null());
        assert_eq!(
            v["readiness"]["missing"],
            json!(["fromTokenDecimals", "acknowledgeTokenRisk"])
        );
        assert_eq!(v["executionAuthorized"], false);
        assert_eq!(v["noDeadlineGranted"], true);
        assert!(crate::trade::okx::validate_swap_options(&a).is_err());
    }

    #[test]
    fn exact_units_and_private_path_omission() {
        let a = args(&[
            "--fromTokenDecimals",
            "9",
            "--acknowledge-token-risk",
            "--okx-min-output-units",
            "123",
            "--okx-config",
            "/nonexistent/private-secret",
        ]);
        let v = intent_summary(&a).unwrap();
        assert_eq!(v["input"]["units"], "9007199254740993123456789");
        assert_eq!(v["output"]["userMinimumUnits"], "123");
        assert_eq!(v["readiness"]["missing"], json!([]));
        assert!(!v.to_string().contains("private-secret"));
        assert!(!v.to_string().contains("sha256"));
        assert_eq!(v["executionAuthorized"], false);
    }

    #[test]
    fn fixed_floor_is_optional_and_explicit_values_remain_visible() {
        let mut a = args(&[
            "--fromTokenDecimals",
            "9",
            "--acknowledge-token-risk",
            "--okx-config",
            "/nonexistent/private-profile",
        ]);
        let v = intent_summary(&a).unwrap();
        assert_eq!(v["readiness"]["missing"], json!([]));
        assert_eq!(v["minimumOutputPolicy"]["mode"], "fresh_route");
        assert!(v["output"]["userMinimumUnits"].is_null());
        assert_eq!(v["executionAuthorized"], false);
        assert!(crate::trade::okx::validate_swap_options(&a).is_ok());
        a.okx_min_output_units = Some("123".into());
        let v = intent_summary(&a).unwrap();
        assert_eq!(
            v["minimumOutputPolicy"]["mode"],
            "fresh_route_plus_fixed_floor"
        );
        assert_eq!(v["minimumOutputPolicy"]["fixedUserMinimumUnits"], "123");
    }

    #[test]
    fn never_rounds_or_expands_parameters() {
        assert!(intent_summary(&args(&["--fromTokenDecimals", "8"])).is_err());
        assert!(intent_summary(&args(&["--okx-min-output-units", "0"])).is_err());
        assert!(intent_summary(&args(&["--okx-min-output-units", "0xff"])).is_err());
        let mut a = args(&[]);
        a.quote.max_to_tax_percent = "100".into();
        assert!(intent_summary(&a).is_err());
        a.quote.max_to_tax_percent = "1.25".into();
        a.mev = "true".into();
        a.gas_level = "HIGH".into();
        let v = intent_summary(&a).unwrap();
        assert_eq!(v["taxCapsPercent"]["output"], "1.25");
        assert_eq!(v["slippagePercent"], "0.5");
        assert_eq!(v["readiness"]["missing"].as_array().unwrap().len(), 4);
        a.quote.from_token = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into();
        a.quote.from_token_decimals = Some(17);
        let v = intent_summary(&a).unwrap();
        assert!(v["readiness"]["missing"]
            .as_array()
            .unwrap()
            .contains(&json!("nativeInputRequires18Decimals")));
    }
}
