//! Prediction markets: discovery, positions, trading. Wire protocol mirrored from `baw`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum PredictionCommand {
    Category {
        #[command(subcommand)]
        command: CategoryCommand,
    },
    Market {
        #[command(subcommand)]
        command: MarketCommand,
    },
    Position {
        #[command(subcommand)]
        command: PositionCommand,
    },
    Order {
        #[command(subcommand)]
        command: OrderCommand,
    },
    Trade {
        #[command(subcommand)]
        command: TradeCommand,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum CategoryCommand {
    List,
}

#[derive(clap::Subcommand, Debug)]
pub enum MarketCommand {
    List,
    Detail,
    Search,
    OrderBook,
    LastTradePrice,
}

#[derive(clap::Subcommand, Debug)]
pub enum PositionCommand {
    List,
    Token,
    SettledHistory,
    Pnl,
    Portfolio,
}

#[derive(clap::Subcommand, Debug)]
pub enum OrderCommand {
    History,
}

#[derive(clap::Subcommand, Debug)]
pub enum TradeCommand {
    Quote,
    PlaceOrder,
    Cancel,
    Redeem,
}

pub fn is_mutation(command: &PredictionCommand) -> bool {
    matches!(
        command,
        PredictionCommand::Trade {
            command: TradeCommand::PlaceOrder | TradeCommand::Cancel | TradeCommand::Redeem
        }
    )
}

pub fn mutation_payload(_command: &PredictionCommand) -> Result<Value, String> {
    Err("prediction: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &PredictionCommand) -> Result<Value, String> {
    Err("prediction: not implemented".into())
}
