//! Limit orders: buy, sell, list, cancel. Wire protocol mirrored from `baw`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum LimitOrderCommand {
    Buy(LimitBuyArgs),
    Sell(LimitSellArgs),
    List(LimitListArgs),
    Cancel(LimitCancelArgs),
}

#[derive(clap::Args, Debug)]
pub struct LimitBuyArgs {}

#[derive(clap::Args, Debug)]
pub struct LimitSellArgs {}

#[derive(clap::Args, Debug)]
pub struct LimitListArgs {}

#[derive(clap::Args, Debug)]
pub struct LimitCancelArgs {}

pub fn is_mutation(command: &LimitOrderCommand) -> bool {
    matches!(command, LimitOrderCommand::Buy(_) | LimitOrderCommand::Sell(_) | LimitOrderCommand::Cancel(_))
}

pub fn mutation_payload(_command: &LimitOrderCommand) -> Result<Value, String> {
    Err("limit-order: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &LimitOrderCommand) -> Result<Value, String> {
    Err("limit-order: not implemented".into())
}
