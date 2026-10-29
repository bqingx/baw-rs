//! Leaderboard, address analysis and alpha radar. Mirrors `baw leaderboard`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Subcommand, Debug)]
pub enum LeaderboardCommand {
    Query,
    Analyze,
    AlphaRadar,
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    AlphaRadarConfig {
        #[command(subcommand)]
        command: RadarConfigCommand,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum PresetCommand {
    List,
    Save,
}

#[derive(clap::Subcommand, Debug)]
pub enum RadarConfigCommand {
    List,
    Save,
}

pub fn is_mutation(command: &LeaderboardCommand) -> bool {
    matches!(
        command,
        LeaderboardCommand::Preset {
            command: PresetCommand::Save
        } | LeaderboardCommand::AlphaRadarConfig {
            command: RadarConfigCommand::Save
        }
    )
}

pub fn mutation_payload(_command: &LeaderboardCommand) -> Result<Value, String> {
    Err("leaderboard: not implemented".into())
}

pub async fn run(_client: &mut Client, _command: &LeaderboardCommand) -> Result<Value, String> {
    Err("leaderboard: not implemented".into())
}
