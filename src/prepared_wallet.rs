//! Private readiness snapshot, never a substitute for backend acceptance or local task gates.
use crate::client::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policy {
    pub(crate) chain: String,
    pub(crate) sender: String,
    pub(crate) developer_mode_expires_at: u64,
    pub(crate) client_binding: String,
}
fn client_binding(client: &Client) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(client.credentials().client_id.as_bytes())
    )
}
impl Policy {
    pub(crate) fn from_ready(proof: &serde_json::Value) -> Result<Self, String> {
        if proof["status"] != "ALLOWANCE_READY" || proof["preparedBeforeTrigger"] != true {
            return Err("prepared wallet record is not ready".into());
        }
        serde_json::from_value(proof["walletPolicy"].clone()).map_err(|_| {
            "prepared wallet policy missing or malformed; no on-trigger reads permitted".into()
        })
    }
    pub(crate) fn capture(
        client: &Client,
        chain: &str,
        sender: &str,
        expires: u64,
    ) -> Result<Self, String> {
        let policy = Self {
            chain: chain.into(),
            sender: sender.into(),
            developer_mode_expires_at: expires,
            client_binding: client_binding(client),
        };
        policy.check(client, chain, Some(sender))?;
        Ok(policy)
    }
    /// A read-only receipt may still be checked after developer mode expires.
    pub(crate) fn sender_on_chain(&self, chain: &str) -> Result<&str, String> {
        if self.chain != chain
            || chain.parse::<u64>().ok().filter(|n| *n > 0).is_none()
            || self.sender.len() != 42
            || !self.sender.starts_with("0x")
            || !self.sender[2..].bytes().all(|b| b.is_ascii_hexdigit())
            || self.sender[2..].bytes().all(|b| b == b'0')
            || self.client_binding.len() != 64
            || !self.client_binding.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid or mismatched prepared wallet policy".into());
        }
        Ok(&self.sender)
    }
    pub(crate) fn check(
        &self,
        client: &Client,
        chain: &str,
        expected: Option<&str>,
    ) -> Result<(String, u64), String> {
        let sender = self.sender_on_chain(chain)?;
        if !client.session_active() || self.client_binding != client_binding(client) {
            return Err("prepared wallet identity/session does not match current client".into());
        }
        if self.developer_mode_expires_at <= chrono::Utc::now().timestamp().max(0) as u64 {
            return Err(
                "prepared developer mode has expired; no on-trigger refresh permitted".into(),
            );
        }
        if expected.is_some_and(|s| !s.eq_ignore_ascii_case(sender)) {
            return Err("sender differs from prepared wallet policy".into());
        }
        Ok((sender.to_owned(), self.developer_mode_expires_at))
    }
}
