//! x402 payments: preview an HTTP 402 challenge, then sign the chosen option.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum X402Command {
    Preview,
    Sign,
}

pub fn is_mutation(command: &X402Command) -> bool {
    matches!(command, X402Command::Sign)
}

pub fn mutation_payload(_command: &X402Command) -> Result<Value, String> {
    Err("x402-payment: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &X402Command) -> Result<Value, String> {
    Err("x402-payment: not implemented".into())
}
