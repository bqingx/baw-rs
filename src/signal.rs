//! Custom signal strategies and backtests. Mirrors `baw signal`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum SignalCommand {
    List,
    Strategy {
        #[command(subcommand)]
        command: StrategyCommand,
    },
    Backtest {
        #[command(subcommand)]
        command: BacktestCommand,
    },
    Explore,
    Credits,
    WalletGroup,
}

#[derive(clap::Subcommand, Debug)]
pub enum StrategyCommand {
    Create,
    Update,
    Delete,
    Follow,
    Unfollow,
    List,
    ListFollowed,
}

#[derive(clap::Subcommand, Debug)]
pub enum BacktestCommand {
    List,
    Detail,
    Retry,
    Schedule,
}

pub fn is_mutation(command: &SignalCommand) -> bool {
    matches!(
        command,
        SignalCommand::Strategy {
            command:
                StrategyCommand::Create
                | StrategyCommand::Update
                | StrategyCommand::Delete
                | StrategyCommand::Follow
                | StrategyCommand::Unfollow
        } | SignalCommand::Backtest {
            command: BacktestCommand::Retry | BacktestCommand::Schedule
        }
    )
}

pub fn mutation_payload(_command: &SignalCommand) -> Result<Value, String> {
    Err("signal: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &SignalCommand) -> Result<Value, String> {
    Err("signal: not implemented".into())
}
