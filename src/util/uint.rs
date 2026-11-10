//! uint256 parsing shared by typed resident calls, native event tasks and provider routes.
/// Accept a raw decimal or 0x-prefixed hex string only; reject signs and overflow.
///
/// Leading zeros are accepted (`"007"`, `"0x0a"`) because the callers of this function never
/// require a canonical spelling; use [`uint256_canonical`] where the exact text matters.
pub fn uint256(s: &str) -> Result<num_bigint::BigUint, String> {
    let (digits, radix, max) = match s.strip_prefix("0x") {
        Some(h) => (h, 16, 64),
        None => (s, 10, 78),
    };
    if digits.is_empty()
        || digits.len() > max
        || !digits.bytes().all(|b| {
            if radix == 16 {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("uint256 must be a raw decimal or 0x hex string, without signs".into());
    }
    let n = num_bigint::BigUint::parse_bytes(digits.as_bytes(), radix).ok_or("invalid uint256")?;
    if n.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(n)
}

/// Strict variant: a canonical decimal string with no sign, padding or hex prefix, so the
/// value can be embedded verbatim in provider calldata and compared with RPC quantities.
pub fn uint256_canonical(s: &str) -> Result<num_bigint::BigUint, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return Err("expected canonical decimal uint256 string".into());
    }
    let n = num_bigint::BigUint::parse_bytes(s.as_bytes(), 10).ok_or("invalid uint256")?;
    if n.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_decimal_and_hex_and_rejects_signs_and_overflow() {
        assert_eq!(uint256("255").unwrap(), uint256("0xff").unwrap());
        assert_eq!(uint256("0").unwrap().bits(), 0);
        // Padding is accepted: the value, not the spelling, is what matters here.
        assert_eq!(uint256("007").unwrap().to_string(), "7");
        assert_eq!(uint256("0x0a").unwrap().to_string(), "10");
        assert_eq!(uint256("0x0000").unwrap().bits(), 0);
        for bad in [
            "",
            "-1",
            "+1",
            " 1",
            "1.0",
            "1e18",
            "0x",
            "0X1",
            "１２",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            assert!(uint256(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn canonical_variant_is_stricter_than_uint256() {
        assert_eq!(uint256_canonical("0").unwrap().bits(), 0);
        assert_eq!(uint256_canonical("7").unwrap().to_string(), "7");
        assert_eq!(
            uint256_canonical(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            )
            .unwrap()
            .bits(),
            256
        );
        // Accepted by the lenient parser, refused here.
        for strict_only in ["007", "00", "0x0a", "0x0"] {
            assert!(
                uint256(strict_only).is_ok(),
                "lenient rejected {strict_only}"
            );
            assert!(
                uint256_canonical(strict_only).is_err(),
                "canonical accepted {strict_only}"
            );
        }
        for bad in [
            "",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1.0",
            "1e18",
            "1_0",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            assert!(uint256_canonical(bad).is_err(), "accepted {bad:?}");
        }
    }
}
