//! Strict uint256 parsing shared by typed resident calls and native event tasks.
/// Accept a raw decimal or 0x-prefixed hex string only; reject signs, padding and overflow.
pub(crate) fn uint256(s: &str) -> Result<num_bigint::BigUint, String> {
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
        return Err(
            "uint256 must be a raw decimal or 0x hex string, without signs or padding".into(),
        );
    }
    let n = num_bigint::BigUint::parse_bytes(digits.as_bytes(), radix).ok_or("invalid uint256")?;
    if n.bits() > 256 {
        return Err("uint256 overflow".into());
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_decimal_and_hex_and_rejects_signs_padding_and_overflow() {
        assert_eq!(uint256("255").unwrap(), uint256("0xff").unwrap());
        assert_eq!(uint256("0").unwrap().bits(), 0);
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
}
