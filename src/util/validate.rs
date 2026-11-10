//! Shared string validators. Each rule keeps the exact accept/reject set of the command that
//! used it before; two rules that merely look similar stay separate named functions.
use sha2::{Digest, Sha256};

/// 1..=64 ASCII letters, digits, `-` or `_`: operation ids, pipeline ids, task ids and wallet
/// names all use this shape. Callers keep their own error wording.
pub(crate) fn operation_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// `8-4-4-4-12` hexadecimal identity shape. Case-insensitive; no version/variant checks.
pub(crate) fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Shape of a session cookie value: at most 16 KiB of visible ASCII without `;` or `,`, which
/// are the header delimiters. An empty value is accepted here; callers that require a
/// non-empty session (for example `Client::set_session`) check that themselves.
pub(crate) fn cookie_value(s: &str) -> bool {
    s.len() <= 16_384
        && s.bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b','))
}

/// Lowercase hexadecimal encoding without a `0x` prefix.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Lowercase SHA-256 digest without a `0x` prefix.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// EVM/Solana chain whitelist for transfer-like commands: only these four values are accepted.
pub(crate) fn chain_id_supported_evm(s: &str) -> Result<(), String> {
    if matches!(s, "1" | "56" | "8453" | "CT_501" | "4663") {
        Ok(())
    } else {
        Err("unsupported binanceChainId".into())
    }
}

/// Portfolio read filters accept any 1..=64 ASCII letter/digit/underscore chain id.
pub(crate) fn chain_id_portfolio(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 64 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        Err("invalid binanceChainId".into())
    } else {
        Ok(())
    }
}

/// Approval commands accept the same alphabet as the portfolio filter but only up to 32 bytes.
pub(crate) fn chain_id_approvals(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("invalid --binanceChainId".into());
    }
    Ok(())
}

/// Speed-up/cancel/resolve pass the chain id through: only empty, padded or control-character
/// values are refused, matching the provider's "auto-detect when omitted" behaviour.
pub(crate) fn chain_id_passthrough(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 32
        || value.chars().any(char::is_control)
    {
        return Err("invalid binanceChainId".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ids_are_bounded_ascii_identifiers() {
        for good in ["a", "intent-123", "A_b-9", &"a".repeat(64)] {
            assert!(operation_id(good), "rejected {good}");
        }
        // `-` alone is accepted: the rule is a character set plus a length bound, and it has
        // always accepted a lone hyphen (wallet names, pipeline ids and operation ids alike).
        assert!(operation_id("-"));
        for bad in ["", "a.b", "a/b", "a b", "é", &"a".repeat(65)] {
            assert!(!operation_id(bad), "accepted {bad:?}");
        }
    }

    #[test]
    fn uuid_shape_is_fixed_width_hexadecimal_with_dashes() {
        assert!(is_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(is_uuid("123E4567-E89B-12D3-A456-426614174000"));
        assert!(!is_uuid(""));
        assert!(!is_uuid("123e4567e89b12d3a456426614174000"));
        assert!(!is_uuid("123e4567-e89b-12d3-a456-42661417400g"));
        assert!(!is_uuid("123e4567_e89b_12d3_a456_426614174000"));
    }

    #[test]
    fn cookie_values_are_bounded_visible_ascii_without_header_delimiters() {
        assert!(cookie_value(""));
        assert!(cookie_value("abc+/=~"));
        assert!(cookie_value(&"a".repeat(16_384)));
        assert!(!cookie_value(&"a".repeat(16_385)));
        assert!(!cookie_value("a;b"));
        assert!(!cookie_value("a,b"));
        assert!(!cookie_value("a b"));
        assert!(!cookie_value("a\nb"));
        assert!(!cookie_value("a\u{7f}b"));
    }

    #[test]
    fn hexadecimal_and_sha256_helpers_are_lowercase_and_unprefixed() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex(&[]), "");
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(sha256_hex(b"abc").len(), 64);
    }

    #[test]
    fn chain_id_rules_stay_distinct() {
        // Transfer-like whitelist: a strict enumerated set, no length/alphabet rule at all.
        for good in ["1", "56", "8453", "CT_501", "4663"] {
            assert!(chain_id_supported_evm(good).is_ok());
        }
        for bad in ["", "0", "137", "ct_501", "56,1", " 56"] {
            assert!(chain_id_supported_evm(bad).is_err(), "accepted {bad:?}");
        }
        // Portfolio: any alphanumeric/underscore id up to 64 bytes.
        assert!(chain_id_portfolio(&format!("A{}", "1".repeat(63))).is_ok());
        assert!(chain_id_portfolio(&"1".repeat(65)).is_err());
        assert!(chain_id_portfolio("56,1").is_err());
        assert!(chain_id_portfolio("").is_err());
        // Approvals: same alphabet, but the limit is 32 bytes.
        assert!(chain_id_approvals(&"1".repeat(32)).is_ok());
        assert!(chain_id_approvals(&"1".repeat(33)).is_err());
        assert!(chain_id_approvals("56-1").is_err());
        // Pass-through leaves the value to the provider but refuses padding/controls.
        assert_eq!(chain_id_passthrough("56").unwrap(), "56");
        assert!(chain_id_passthrough("CT_501").is_ok());
        assert!(chain_id_passthrough(" 56").is_err());
        assert!(chain_id_passthrough("56\n").is_err());
        assert!(chain_id_passthrough(&"1".repeat(33)).is_err());
        assert!(chain_id_passthrough("").is_err());
    }
}
