//! Local self-checks the JavaScript CLI ships: `cli-check` and `skill-check`.
use crate::client::Client;
use serde_json::Value;

#[derive(clap::Args, Debug)]
pub struct CliCheckArgs {
    /// Required CLI version
    #[arg(long = "required-version")]
    pub required_version: String,
}

#[derive(clap::Args, Debug)]
pub struct SkillCheckArgs {
    /// Skill name to check
    #[arg(long = "skill-name")]
    pub skill_name: String,
    /// Current version to compare against
    #[arg(long = "current-version")]
    pub current_version: String,
}

pub async fn cli_check(_client: &mut Client, _args: &CliCheckArgs) -> Result<Value, String> {
    Err("cli-check: not implemented".into())
}

pub async fn skill_check(_client: &mut Client, _args: &SkillCheckArgs) -> Result<Value, String> {
    Err("skill-check: not implemented".into())
}
