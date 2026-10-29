use serde_json::Value;

pub fn unpack_response(status: u16, body: &[u8]) -> Result<Value, String> {
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "invalid JSON response".to_string())?;
    if value.get("code").and_then(Value::as_str) != Some("000000") {
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 16 && s.bytes().all(|b| b.is_ascii_digit()))
            .unwrap_or("unknown");
        return Err(format!("Binance API code {code}"));
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| "missing API data".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_failed_or_malformed_envelopes_without_echoing_secret_values() {
        for (status, body) in [
            (401, br#"{"data":{"secret":"sensitive"}}"#.as_slice()),
            (
                200,
                br#"{"code":"100001005","message":"sensitive","data":null}"#.as_slice(),
            ),
            (200, br#"{"data":{}}"#.as_slice()),
            (200, b"not json".as_slice()),
        ] {
            let err = unpack_response(status, body).expect_err("must fail closed");
            assert!(!err.contains("sensitive"));
        }
    }
    #[test]
    fn successful_api_response_returns_only_data() {
        let got =
            unpack_response(200, br#"{"code":"000000","data":{"status":"CONNECTED"}}"#).unwrap();
        assert_eq!(got["status"], "CONNECTED");
    }
}
