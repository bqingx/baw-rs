//! Wallet operations the JavaScript CLI exposes beyond query: speed-up, cancel,
//! resolve-domain.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Args, Debug)]
pub struct SpeedUpArgs {}

#[derive(clap::Args, Debug)]
pub struct CancelTxArgs {}

#[derive(clap::Args, Debug)]
pub struct ResolveDomainArgs {}

pub async fn speed_up(_client: &mut Client, _args: &SpeedUpArgs) -> Result<Value, String> {
    Err("wallet speed-up: not implemented".into())
}

pub fn speed_up_payload(_args: &SpeedUpArgs) -> Result<Value, String> {
    Err("wallet speed-up: not implemented".into())
}

pub fn cancel_payload(_args: &CancelTxArgs) -> Result<Value, String> {
    Err("wallet cancel: not implemented".into())
}

pub async fn cancel(_client: &mut Client, _args: &CancelTxArgs) -> Result<Value, String> {
    Err("wallet cancel: not implemented".into())
}

pub async fn resolve_domain(_client: &mut Client, _args: &ResolveDomainArgs) -> Result<Value, String> {
    Err("wallet resolve-domain: not implemented".into())
}
