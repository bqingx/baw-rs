//! Synthetic fixtures shared by pipeline runtime/recovery tests, not an execution path.
use serde_json::{json, Value};
pub(super) const SENDER: &str = "0x1111111111111111111111111111111111111111";
pub(super) fn valid_preview(id: &str) -> Value {
    json!({"requestId":id,"from":SENDER,"binanceChainId":"56","simulationSuccess":true,"expiresAt":4102444800u64,"risks":{"riskDetails":[]}})
}
