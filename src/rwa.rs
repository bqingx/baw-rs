//! Command-local RWA registry and exact decimal share arithmetic.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_rounding_and_large_amounts() {
        assert_eq!(
            multiply_down("184467440737095516160.1234567891234567899", "2", 18).unwrap(),
            "368934881474191032320.246913578246913579"
        );
        assert_eq!(divide_up("1", "3", 18).unwrap(), "0.333333333333333334");
        assert!(multiply_down("1", "NaN", 18).is_err());
        assert!(divide_up("1", "0", 18).is_err());
    }
}
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use std::str::FromStr;
pub(crate) fn decimal(s: &str) -> Result<BigDecimal, String> {
    if s.len() > 10000 || s.trim() != s || s.is_empty() {
        return Err("invalid decimal".into());
    }
    let n = BigDecimal::from_str(s).map_err(|_| "invalid finite decimal")?;
    if n.as_bigint_and_exponent().1.unsigned_abs() > 10000 {
        return Err("decimal scale out of range".into());
    }
    Ok(n)
}
pub(crate) fn text(n: BigDecimal) -> String {
    n.normalized().to_plain_string()
}
fn operands(a: &str, m: &str, scale: i64) -> Result<(BigDecimal, BigDecimal), String> {
    if !(0..=10000).contains(&scale) {
        return Err("invalid decimal scale".into());
    }
    let a = decimal(a)?;
    let m = decimal(m)?;
    if m <= 0 {
        return Err("multiplier must be positive and finite".into());
    }
    Ok((a, m))
}
pub fn multiply_down(amount: &str, multiplier: &str, scale: i64) -> Result<String, String> {
    let (a, m) = operands(amount, multiplier, scale)?;
    Ok(text((a * m).with_scale(scale)))
}
pub fn divide_up(amount: &str, multiplier: &str, scale: i64) -> Result<String, String> {
    divide(amount, multiplier, scale, true)
}
pub(crate) fn divide(
    amount: &str,
    multiplier: &str,
    scale: i64,
    up: bool,
) -> Result<String, String> {
    let (a, m) = operands(amount, multiplier, scale)?;
    let (mut numerator, sa) = a.as_bigint_and_exponent();
    let (mut denominator, sm) = m.as_bigint_and_exponent();
    let shift = sm - sa + scale;
    if shift >= 0 {
        numerator *= BigInt::from(10u8).pow(shift as u32);
    } else {
        denominator *= BigInt::from(10u8).pow((-shift) as u32);
    }
    let mut q = &numerator / &denominator;
    if up && &numerator % &denominator != BigInt::from(0) {
        q += if numerator < BigInt::from(0) { -1 } else { 1 };
    }
    Ok(text(BigDecimal::new(q, scale)))
}

#[derive(Clone, Debug)]
pub struct RwaToken {
    pub multiplier: String,
    pub kind: String,
    pub rwa_type: Option<i64>,
}
#[derive(Clone, Debug, Default)]
pub struct Registry {
    tokens: std::collections::HashMap<(String, String), RwaToken>,
}
impl Registry {
    pub(crate) fn from_rows(rows: &serde_json::Value) -> Self {
        let mut registry = Self::default();
        for row in rows.as_array().into_iter().flatten() {
            let (Some(chain), Some(address), Some(multiplier)) = (
                row["chainId"].as_str(),
                row["contractAddress"].as_str(),
                row["multiplier"].as_str(),
            ) else {
                continue;
            };
            if chain.is_empty()
                || chain.trim() != chain
                || address.is_empty()
                || address.trim() != address
                || decimal(multiplier).map(|n| n <= 0).unwrap_or(true)
            {
                continue;
            }
            let rwa_type = row["type"].as_i64();
            let kind = match rwa_type {
                Some(1) => "ondo",
                Some(3) => "bstock",
                _ => "unknown",
            }
            .into();
            registry.tokens.insert(
                (chain.into(), identity(chain, address)),
                RwaToken {
                    multiplier: multiplier.into(),
                    kind,
                    rwa_type,
                },
            );
        }
        registry
    }
    pub fn token(&self, chain: &str, address: &str) -> Option<&RwaToken> {
        self.tokens.get(&(chain.into(), identity(chain, address)))
    }
}
#[cfg(test)]
mod registry_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn registry_validation_and_chain_specific_identity() {
        let r = Registry::from_rows(&json!([
            {"chainId":"56","contractAddress":"0xAbC","multiplier":"1.25","type":1},
            {"chainId":"CT_501","contractAddress":"AbC","multiplier":"2","type":3},
            {"chainId":"1","contractAddress":"0xAbC","multiplier":"3","type":99},
            {"chainId":"56","contractAddress":"bad","multiplier":"0"},
            {"chainId":"56","contractAddress":"bad2","multiplier":"NaN"},
            {"chainId":"56","contractAddress":"bad3","multiplier":"-1"},
            {"chainId":"","contractAddress":"bad4","multiplier":"1"}
        ]));
        assert_eq!(r.token("56", "0xabc").unwrap().kind, "ondo");
        assert_eq!(r.token("CT_501", "AbC").unwrap().kind, "bstock");
        assert!(r.token("CT_501", "abc").is_none());
        assert_eq!(r.token("1", "0xabc").unwrap().kind, "unknown");
        assert!(r.token("56", "bad").is_none());
        assert!(r.token("56", "bad2").is_none());
        assert!(r.token("56", "bad3").is_none());
        assert!(r.token("137", "0xabc").is_none());
    }
}

pub(crate) fn identity(chain: &str, address: &str) -> String {
    if chain == "CT_501" {
        address.into()
    } else if address.starts_with("0x") || address.starts_with("0X") {
        address.to_ascii_lowercase()
    } else {
        address.into()
    }
}

/// Fetch once per caller; deliberately no global or persistent registry cache.
pub async fn fetch(client: &mut crate::client::Client) -> Result<Registry, String> {
    let rows = client
        .request(
            reqwest::Method::GET,
            "/bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list",
            None,
        )
        .await?;
    if !rows.is_array() {
        return Err("invalid RWA registry response".into());
    }
    Ok(Registry::from_rows(&rows))
}
