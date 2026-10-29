//! Pre-authorized execution policy for unattended `contract-call run`.
//!
//! A policy pins the exact call shape. Anything that does not match byte-for-byte
//! is refused *before* a preview is requested, so an unattended run can never be
//! redirected to a different contract, selector or amount.
use serde_json::Value;
use std::path::Path;

pub const MAX_POLICY_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub version: u32,
    pub chain_id: String,
    pub to: String,
    pub value_wei: String,
    pub selector: Option<String>,
    pub calldata_sha256: Option<String>,
    pub max_gas_limit: Option<u64>,
    pub allow_risks: Vec<String>,
    pub allow_authority_changes: bool,
    pub max_total_wei: Option<String>,
}

fn hex_sha(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn selector(v: &str) -> bool {
    v.len() == 10 && v.starts_with("0x") && v[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn uint(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
}

impl Policy {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(raw).map_err(|_| "policy is not valid JSON")?;
        if !v.is_object() {
            return Err("policy must be a JSON object".into());
        }
        let version = v["version"].as_u64().ok_or("policy version missing")?;
        let version = u32::try_from(version).map_err(|_| "policy version out of range")?;
        if version != 1 {
            return Err("unsupported policy version".into());
        }
        let chain_id = v["chainId"]
            .as_str()
            .ok_or("policy chainId missing")?
            .to_owned();
        if chain_id.is_empty() {
            return Err("policy chainId empty".into());
        }
        let to = v["to"].as_str().ok_or("policy to missing")?.to_owned();
        if !to.starts_with("0x") || to.len() != 42 {
            return Err("policy to must be a 0x address".into());
        }
        let value_wei = v["valueWei"].as_str().unwrap_or("0").to_owned();
        if !uint(&value_wei) {
            return Err("policy valueWei must be an unsigned integer string".into());
        }
        let selector = match v.get("selector") {
            None | Some(Value::Null) => None,
            Some(s) => {
                let s = s.as_str().ok_or("policy selector must be a string")?;
                if !selector(s) {
                    return Err("policy selector must be 0x + 4 bytes".into());
                }
                Some(s.to_ascii_lowercase())
            }
        };
        let calldata_sha256 = match v.get("calldataSha256") {
            None | Some(Value::Null) => None,
            Some(s) => {
                let s = s.as_str().ok_or("policy calldataSha256 must be a string")?;
                if !hex_sha(s) {
                    return Err("policy calldataSha256 must be 64 lowercase hex".into());
                }
                Some(s.to_owned())
            }
        };
        let max_gas_limit = match v.get("maxGasLimit") {
            None | Some(Value::Null) => None,
            Some(n) => Some(n.as_u64().ok_or("policy maxGasLimit must be an integer")?),
        };
        let allow_risks = match v.get("allowRisks") {
            None | Some(Value::Null) => Vec::new(),
            Some(a) => a
                .as_array()
                .ok_or("policy allowRisks must be an array")?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "policy allowRisks entries must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let allow_authority_changes = v["allowAuthorityChanges"].as_bool().unwrap_or(false);
        let max_total_wei = match v.get("maxTotalWei") {
            None | Some(Value::Null) => None,
            Some(s) => {
                let s = s.as_str().ok_or("policy maxTotalWei must be a string")?;
                if !uint(s) {
                    return Err("policy maxTotalWei must be an unsigned integer string".into());
                }
                Some(s.to_owned())
            }
        };
        Ok(Self {
            version,
            chain_id,
            to,
            value_wei,
            selector,
            calldata_sha256,
            max_gas_limit,
            allow_risks,
            allow_authority_changes,
            max_total_wei,
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|_| "cannot read policy file")?;
        if !meta.is_file() || meta.len() > MAX_POLICY_BYTES {
            return Err("policy must be a regular file under 16 KiB".into());
        }
        let raw = std::fs::read_to_string(path).map_err(|_| "cannot read policy file")?;
        Self::parse(&raw)
    }
}

/// Everything the caller wants to do, already validated as a call shape.
pub struct Intent<'a> {
    pub chain_id: &'a str,
    pub to: &'a str,
    pub value: &'a str,
    pub input_data: Option<&'a str>,
    pub gas_limit: Option<u64>,
}

impl Policy {
    pub fn check_intent(&self, intent: &Intent<'_>) -> Result<(), String> {
        if self.chain_id != intent.chain_id {
            return Err("policy chain mismatch".into());
        }
        if !self.to.eq_ignore_ascii_case(intent.to) {
            return Err("policy target contract mismatch".into());
        }
        let value = normalize_uint(intent.value)?;
        if value != self.value_wei {
            return Err("policy value mismatch".into());
        }
        if let Some(want) = &self.selector {
            let data = intent.input_data.ok_or("policy requires inputData")?;
            if data.len() < 10 || !data[..10].eq_ignore_ascii_case(want) {
                return Err("policy selector mismatch".into());
            }
        }
        if let Some(want) = &self.calldata_sha256 {
            let data = intent.input_data.ok_or("policy requires inputData")?;
            if &sha256_hex(data.as_bytes()) != want {
                return Err("policy calldata digest mismatch".into());
            }
        }
        if let Some(max) = self.max_gas_limit {
            match intent.gas_limit {
                Some(g) if g <= max => {}
                Some(_) => return Err("policy gas limit exceeded".into()),
                None => return Err("policy requires an explicit gasLimit".into()),
            }
        }
        if self.max_total_wei.is_some() && value != "0" {
            return Err("policy maxTotalWei only supports zero-value calls".into());
        }
        Ok(())
    }

    /// Risk gate: unknown risk codes and any authority change abort the run.
    pub fn check_preview(&self, preview: &Value) -> Result<(), String> {
        let empty = Vec::new();
        let details = preview["risks"]["riskDetails"].as_array().unwrap_or(&empty);
        for item in details {
            let code = item["code"].as_str().unwrap_or("");
            if !self.allow_risks.iter().any(|allowed| allowed == code) {
                return Err(format!("preview risk {code} is not allowed by policy"));
            }
        }
        for key in ["authorityChanges", "allowanceChanges"] {
            let changes = &preview[key];
            let nonempty = match changes {
                Value::Null => false,
                Value::Array(a) => !a.is_empty(),
                Value::Object(o) => !o.is_empty(),
                _ => true,
            };
            if nonempty && !self.allow_authority_changes {
                return Err(format!("preview {key} not allowed by policy"));
            }
        }
        Ok(())
    }
}

fn normalize_uint(v: &str) -> Result<String, String> {
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("value must be unsigned decimal or hex wei".into());
        }
        let n = u128::from_str_radix(hex, 16).map_err(|_| "value out of range")?;
        return Ok(n.to_string());
    }
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err("value must be unsigned decimal or hex wei".into());
    }
    let trimmed = v.trim_start_matches('0');
    Ok(if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Policy {
        Policy::parse(
            &json!({
                "version": 1,
                "chainId": "56",
                "to": "0x1111111111111111111111111111111111111111",
                "valueWei": "0",
                "selector": "0xdeadbeef",
                "maxGasLimit": 300000,
                "allowRisks": [],
                "allowAuthorityChanges": false
            })
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn accepts_only_the_exact_pinned_call_shape() {
        let p = base();
        let ok = Intent {
            chain_id: "56",
            to: "0x1111111111111111111111111111111111111111",
            value: "0x0",
            input_data: Some("0xdeadbeef00"),
            gas_limit: Some(250000),
        };
        assert!(p.check_intent(&ok).is_ok());
        let wrong_chain = Intent {
            chain_id: "1",
            ..ok
        };
        assert!(p.check_intent(&wrong_chain).is_err());
    }

    #[test]
    fn rejects_wrong_target_selector_value_and_oversized_gas() {
        let p = base();
        let mk =
            |to: &'static str, value: &'static str, data: &'static str, gas: Option<u64>| Intent {
                chain_id: "56",
                to,
                value,
                input_data: Some(data),
                gas_limit: gas,
            };
        assert!(p
            .check_intent(&mk(
                "0x2222222222222222222222222222222222222222",
                "0",
                "0xdeadbeef",
                Some(21000)
            ))
            .is_err());
        assert!(p
            .check_intent(&mk(
                "0x1111111111111111111111111111111111111111",
                "1",
                "0xdeadbeef",
                Some(21000)
            ))
            .is_err());
        assert!(p
            .check_intent(&mk(
                "0x1111111111111111111111111111111111111111",
                "0",
                "0xfeedface",
                Some(21000)
            ))
            .is_err());
        assert!(p
            .check_intent(&mk(
                "0x1111111111111111111111111111111111111111",
                "0",
                "0xdeadbeef",
                Some(400000)
            ))
            .is_err());
        assert!(p
            .check_intent(&mk(
                "0x1111111111111111111111111111111111111111",
                "0",
                "0xdeadbeef",
                None
            ))
            .is_err());
    }

    #[test]
    fn blocks_unlisted_risks_and_authority_changes() {
        let p = base();
        let risky =
            json!({"risks": {"riskDetails": [{"code": "RISK_001"}]}, "authorityChanges": []});
        assert!(p.check_preview(&risky).is_err());
        let auth = json!({"risks": {"riskDetails": []}, "authorityChanges": [{"token": "0x1", "amount": "1"}]});
        assert!(p.check_preview(&auth).is_err());
        let clean =
            json!({"risks": {"riskDetails": []}, "authorityChanges": [], "allowanceChanges": []});
        assert!(p.check_preview(&clean).is_ok());
    }

    #[test]
    fn calldata_digest_is_enforced_when_present() {
        let raw = json!({
            "version": 1,
            "chainId": "56",
            "to": "0x1111111111111111111111111111111111111111",
            "valueWei": "0",
            "calldataSha256": sha256_hex(b"0xdeadbeef")
        })
        .to_string();
        let p = Policy::parse(&raw).unwrap();
        let good = Intent {
            chain_id: "56",
            to: "0x1111111111111111111111111111111111111111",
            value: "0",
            input_data: Some("0xdeadbeef"),
            gas_limit: None,
        };
        assert!(p.check_intent(&good).is_ok());
        let bad = Intent {
            input_data: Some("0xdeadbeee"),
            ..good
        };
        assert!(p.check_intent(&bad).is_err());
    }

    #[test]
    fn rejects_malformed_policy_documents() {
        for raw in [
            "not json",
            "[]",
            r#"{"version":2,"chainId":"56","to":"0x1111111111111111111111111111111111111111"}"#,
            r#"{"version":1,"chainId":"","to":"0x1111111111111111111111111111111111111111"}"#,
            r#"{"version":1,"chainId":"56","to":"0x11"}"#,
            r#"{"version":1,"chainId":"56","to":"0x1111111111111111111111111111111111111111","valueWei":"-1"}"#,
            r#"{"version":1,"chainId":"56","to":"0x1111111111111111111111111111111111111111","selector":"0x12"}"#,
            r#"{"version":1,"chainId":"56","to":"0x1111111111111111111111111111111111111111","calldataSha256":"abc"}"#,
        ] {
            assert!(Policy::parse(raw).is_err(), "accepted {raw}");
        }
    }
}
