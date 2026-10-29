//! Token approvals: list, detail, revoke. Wire protocol mirrored from `baw`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum ApprovalsCommand {
    List(ApprovalsListArgs),
    Detail(ApprovalsDetailArgs),
    Revoke(ApprovalsRevokeArgs),
}

#[derive(clap::Args, Debug)]
pub struct ApprovalsListArgs {}

#[derive(clap::Args, Debug)]
pub struct ApprovalsDetailArgs {}

#[derive(clap::Args, Debug)]
pub struct ApprovalsRevokeArgs {}

pub fn is_mutation(command: &ApprovalsCommand) -> bool {
    matches!(command, ApprovalsCommand::Revoke(_))
}

pub fn mutation_payload(_command: &ApprovalsCommand) -> Result<Value, String> {
    Err("approvals: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &ApprovalsCommand) -> Result<Value, String> {
    Err("approvals: not implemented".into())
}
