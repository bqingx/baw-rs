//! Wallet tracker: groups, addresses, monitors. Mirrors `baw tracker`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum TrackerCommand {
    Token,
    Tx,
    Follow,
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    Address {
        #[command(subcommand)]
        command: AddressCommand,
    },
    Ws,
}

#[derive(clap::Subcommand, Debug)]
pub enum GroupCommand {
    Create,
    List,
    Update,
}

#[derive(clap::Subcommand, Debug)]
pub enum AddressCommand {
    Add,
    Batch,
    Delete,
    Follow,
    Link,
    List,
    Search,
    Unfollow,
    Update,
}

pub fn is_mutation(command: &TrackerCommand) -> bool {
    matches!(
        command,
        TrackerCommand::Group { .. }
            | TrackerCommand::Address {
                command:
                    AddressCommand::Add
                    | AddressCommand::Batch
                    | AddressCommand::Delete
                    | AddressCommand::Follow
                    | AddressCommand::Link
                    | AddressCommand::Unfollow
                    | AddressCommand::Update
            }
    )
}

pub fn mutation_payload(_command: &TrackerCommand) -> Result<Value, String> {
    Err("tracker: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &TrackerCommand) -> Result<Value, String> {
    Err("tracker: not implemented".into())
}
