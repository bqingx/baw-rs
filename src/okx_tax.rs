//! Fee-on-transfer metadata limits, not a prediction of execution or net output.
use super::{amount_units, display_units, is_native, percentage, QuoteArgs};
use num_bigint::BigUint;
use serde_json::{json, Value};

pub(super) fn validate_caps(q: &QuoteArgs) -> Result<(), String> {
    cap(&q.max_from_tax_percent, "--max-from-tax-percent")?;
    cap(&q.max_to_tax_percent, "--max-to-tax-percent")?;
    Ok(())
}

fn cap(raw: &str, flag: &str) -> Result<BigUint, String> {
    percentage(raw)
        .map_err(|e| format!("{flag}: expected 0..<100 percent with at most 8 decimal places: {e}"))
}

pub(super) struct Taxes {
    pub from_rate: BigUint,
    pub to_rate: BigUint,
    from_raw: String,
    to_raw: String,
}

impl Taxes {
    pub(super) fn from_quote(q: &QuoteArgs, row: &Value) -> Result<Self, String> {
        let from_cap = cap(&q.max_from_tax_percent, "--max-from-tax-percent")?;
        let to_cap = cap(&q.max_to_tax_percent, "--max-to-tax-percent")?;
        let (from_rate, from_raw) = rate(
            row,
            "fromToken",
            &q.from_token,
            &from_cap,
            "--max-from-tax-percent",
        )?;
        let (to_rate, to_raw) = rate(row, "toToken", &q.to_token, &to_cap, "--max-to-tax-percent")?;
        Ok(Self {
            from_rate,
            to_rate,
            from_raw,
            to_raw,
        })
    }

    pub(super) fn any(&self) -> bool {
        self.from_rate.bits() != 0 || self.to_rate.bits() != 0
    }

    pub(super) fn summary(&self, q: &QuoteArgs) -> Value {
        json!({
            "taxed":self.any(),
            "quoteAndReportedMinimumTaxInclusion":"unspecified_by_provider_not_recomputed",
            "fromToken": {
                "taxRate": self.from_raw,
                "taxPercent": display_units(&(&self.from_rate * 100u32), 18),
                "maxTaxPercent": q.max_from_tax_percent,
            },
            "toToken": {
                "taxRate": self.to_raw,
                "taxPercent": display_units(&(&self.to_rate * 100u32), 18),
                "maxTaxPercent": q.max_to_tax_percent,
            },
            "warning": "Reported tax metadata only, not guarantees of actual tax, execution support, or net output. Rates can change.",
        })
    }
}

fn rate(
    row: &Value,
    side: &str,
    token: &str,
    cap: &BigUint,
    flag: &str,
) -> Result<(BigUint, String), String> {
    let raw = row
        .get(side)
        .and_then(|metadata| metadata.get("taxRate"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("OKX {side}.taxRate must be a known exact decimal string"))?;
    let rate =
        amount_units(raw, 18, false).map_err(|e| format!("OKX invalid {side}.taxRate: {e}"))?;
    if rate >= BigUint::from(10u32).pow(18) {
        return Err(format!("OKX {side}.taxRate must be a fraction in 0..<1"));
    }
    if rate.bits() != 0 && is_native(token) {
        return Err(format!(
            "OKX nonzero {side}.taxRate contradicts native asset"
        ));
    }
    // Percentage cap is scaled by 1e8; converting to a fraction scaled by
    // 1e18 multiplies by 1e8 (1e18 / 100 / 1e8).
    if rate > cap * BigUint::from(10u32).pow(8) {
        return Err(format!(
            "OKX {side}.taxRate exceeds {flag}; nonzero tax requires explicit opt-in via {flag}"
        ));
    }
    Ok((rate, raw.to_owned()))
}

#[cfg(test)]
#[path = "okx_tax_tests.rs"]
mod tests;
