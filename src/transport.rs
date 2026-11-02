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
        return Err(match explained_message(code, &value) {
            Some(m) => format!("Binance API code {code}: {m}"),
            None => format!("Binance API code {code}"),
        });
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| "missing API data".into())
}

/// The backend's `message` is what distinguishes "ran out of gas" from "slippage" behind an opaque
/// numeric code, and a rejected preview cannot be diagnosed without it. Only codes whose text is
/// operator-facing guidance about the transaction itself are explained; session-binding
/// (`351801`/`351802`), risk-control (`351803`), balance-cap (`351804`) and every non-`3518` code
/// keep the bare code, because that text can quote session or account material. Even for the
/// explained codes, anything credential-shaped is replaced and the result is bounded.
fn explained_message(code: &str, v: &Value) -> Option<String> {
    const EXPLAINED: [&str; 5] = ["351805", "351806", "351807", "351818", "351821"];
    if !EXPLAINED.contains(&code) {
        return None;
    }
    let raw = v.get("message").and_then(Value::as_str)?;
    let mut out = String::new();
    for word in raw.split_whitespace() {
        let clean: String = word.chars().filter(|c| !c.is_control()).collect();
        if clean.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(if credential_shaped(&clean) {
            "<redacted>"
        } else {
            &clean
        });
    }
    if out.is_empty() {
        return None;
    }
    Some(out.chars().take(200).collect())
}

/// Conservative shape test for anything that could carry a secret. JWT and base64url cookies contain
/// dots; hex session ids and API keys are plain letters+digits; query strings carry `key=value`. The
/// rule is deliberately over-eager: redacting a number from an error message costs nothing, leaking a
/// session token into a persisted operation record costs everything.
fn credential_shaped(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    let len = chars.len();
    let all = |set: &str| {
        chars
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || set.contains(*c))
    };
    let has_digit = chars.iter().any(char::is_ascii_digit);
    let has_alpha = chars.iter().any(char::is_ascii_alphabetic);
    // key=value where the value is long enough to be a credential
    if let Some((_, value)) = word.split_once('=') {
        if value.chars().count() >= 8 {
            return true;
        }
    }
    // JWTs / base64url blobs with separators
    if len >= 16 && all("._-+/=") && has_digit && has_alpha {
        return true;
    }
    // long unbroken base64-ish or hex-ish runs
    if len >= 24 && all("+/=_-") {
        return true;
    }
    if len >= 16 && all("") && has_digit && has_alpha {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_failed_or_malformed_envelopes_without_echoing_secret_values() {
        for (status, body) in [
            (401, br#"{"data":{"secret":"sensitive"}}"#.as_slice()),
            (200, br#"{"code":"100001005","data":null}"#.as_slice()),
            (200, br#"{"data":{}}"#.as_slice()),
            (200, b"not json".as_slice()),
        ] {
            let err = unpack_response(status, body).expect_err("must fail closed");
            assert!(!err.contains("sensitive"));
        }
        // A code without a message stays a bare code: nothing from the body is echoed.
        assert_eq!(
            unpack_response(200, br#"{"code":"351805","data":null}"#).unwrap_err(),
            "Binance API code 351805"
        );
        // Only the preview/simulation family is explained; auth and session text is never echoed.
        assert_eq!(
            unpack_response(
                200,
                br#"{"code":"123456","message":"secret-session-value","data":null}"#
            )
            .unwrap_err(),
            "Binance API code 123456"
        );
    }

    #[test]
    fn backend_message_is_kept_so_a_rejection_is_diagnosable_but_credentials_are_not() {
        // The explanation is the whole point: "ran out of gas" tells an operator what to change,
        // a bare 351805 does not.
        let err = unpack_response(
            200,
            br#"{"code":"351805","message":"The transaction ran out of gas. Please increase the gas limit and try again.","data":null}"#,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "Binance API code 351805: The transaction ran out of gas. Please increase the gas limit and try again."
        );
        // Credential-shaped runs never reach the record: plain runs, JWTs, hex ids, query strings,
        // and Bearer values are all replaced, and the message stays bounded.
        for secret in [
            "A".repeat(64),
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVP".to_string(),
            "9afc68087a5959794c38ceb93".to_string(),
            "apikey=ABC123456789&sig=deadbeefcafe".to_string(),
            "YWJjZGVmZ2hpamtsbW5vcA".to_string(),
        ] {
            let err = unpack_response(
                200,
                format!(r#"{{"code":"351805","message":"failed for {secret}","data":null}}"#)
                    .as_bytes(),
            )
            .unwrap_err();
            assert!(!err.contains(&secret), "secret echoed: {err}");
            assert!(err.contains("<redacted>"), "not redacted: {err}");
        }
        let long = "word ".repeat(200);
        let err = unpack_response(
            200,
            format!(r#"{{"code":"351805","message":"{long}","data":null}}"#).as_bytes(),
        )
        .unwrap_err();
        assert!(err.len() <= "Binance API code 351805: ".len() + 200);
    }

    #[test]
    fn session_binding_and_risk_codes_are_never_explained() {
        // 351801/351802 bind a preview to a session, 351803 is risk control and 351804 quotes the
        // wallet balance: their text can carry session or account material, so only the code appears.
        for code in ["351801", "351802", "351803", "351804", "1200009", "3518"] {
            let err = unpack_response(
                200,
                format!(
                    r#"{{"code":"{code}","message":"session eyJhbGciOiJIUzI1NiJ9.abcdefghij","data":null}}"#
                )
                .as_bytes(),
            )
            .unwrap_err();
            assert_eq!(
                err,
                format!("Binance API code {code}"),
                "code {code} explained"
            );
        }
    }
    #[test]
    fn successful_api_response_returns_only_data() {
        let got =
            unpack_response(200, br#"{"code":"000000","data":{"status":"CONNECTED"}}"#).unwrap();
        assert_eq!(got["status"], "CONNECTED");
    }
}
