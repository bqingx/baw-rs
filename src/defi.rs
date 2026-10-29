//! DeFi: protocols, opportunities, positions, deposit/redeem/LP/claim. Mirrors `baw defi`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum DefiCommand {
    ProtocolList,
    ProtocolInfo,
    InvestmentList,
    InvestmentInfo,
    Position,
    Deposit,
    Redeem,
    LpAdd,
    LpRemove,
    Claim,
    Preview,
}

pub fn is_mutation(command: &DefiCommand) -> bool {
    matches!(
        command,
        DefiCommand::Deposit
            | DefiCommand::Redeem
            | DefiCommand::LpAdd
            | DefiCommand::LpRemove
            | DefiCommand::Claim
    )
}

pub fn mutation_payload(_command: &DefiCommand) -> Result<Value, String> {
    Err("defi: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &DefiCommand) -> Result<Value, String> {
    Err("defi: not implemented".into())
}
