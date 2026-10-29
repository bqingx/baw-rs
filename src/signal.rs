//! Custom signal strategies and backtests. Mirrors `baw signal` 1.12.0.
//!
//! Every endpoint and payload below is taken verbatim from the installed bundle
//! `/usr/lib/node_modules/@binance/agentic-wallet/dist/index.js`. The API class is
//! `var Dn=class t extends E{...}` with
//!
//! ```text
//! static BASE_PATH="/bapi/defi/v1/public/wallet-direct/agent/custom-signal"
//! static MEME_PATH="/bapi/defi/v1/public/wallet-direct/custom-signal/meme/query"
//! static SM_PATH="/bapi/defi/v1/public/wallet-direct/buw/wallet/web/signal/smart-money"
//! ```
//!
//! None of those prefixes live under `client::API`, so the full path is used. The
//! service layer is `var G=class{...}` and the command tree is built by `Zi` (list),
//! `ns` (strategy), `rs` (backtest), `os` (explore), `is` (credits) and `ss`
//! (wallet-group).
//!
//! Reads (`list`, `strategy list`, `strategy list-followed`, `backtest list`,
//! `backtest detail`, `explore`, `credits`, `wallet-group`) are still POSTs on the
//! wire: the backend takes JSON bodies for queries. Writes are
//! `strategy create|update|delete|follow|unfollow` and `backtest retry|schedule`;
//! `backtest schedule` without `--interval` only queries, but the leaf is classified
//! as a write by the family classification and stays inside the consent gate.
//!
//! Interactive prompts (`-y, --yes`) have no TTY equivalent here, so the default-deny
//! answer is enforced instead: a leaf that the JS CLI would abort after answering the
//! prompt with the default `N` returns a static error until `--yes` is passed. Global
//! `--confirm`/`--operation-id` remain the actual consent gate in `dispatch.rs`.
use crate::client::Client;
use reqwest::Method;
use serde_json::{json, Map, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Bundle: `static BASE_PATH="/bapi/defi/v1/public/wallet-direct/agent/custom-signal"`.
const BASE_PATH: &str = "/bapi/defi/v1/public/wallet-direct/agent/custom-signal";
/// Bundle: `static MEME_PATH="/bapi/defi/v1/public/wallet-direct/custom-signal/meme/query"`.
const MEME_PATH: &str = "/bapi/defi/v1/public/wallet-direct/custom-signal/meme/query";
/// Bundle: `static SM_PATH="/bapi/defi/v1/public/wallet-direct/buw/wallet/web/signal/smart-money"`.
const SMART_MONEY_PATH: &str =
    "/bapi/defi/v1/public/wallet-direct/buw/wallet/web/signal/smart-money";
/// Bundle: `this.request("/bapi/defi/v1/public/wallet-direct/custom-signal/strategy/explore",...)`.
const EXPLORE_PATH: &str = "/bapi/defi/v1/public/wallet-direct/custom-signal/strategy/explore";
/// Bundle: `this.request("/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet/group/list",...)`.
const WALLET_GROUP_PATH: &str =
    "/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet/group/list";

/// Bundle: `function re(t){...parseInt...isNaN(e)||e<1||e>nt?null:e}` with `nt=100`.
const PAGE_SIZE_MAX: u32 = 100;
/// Bundle: `function Ge(t){...isNaN(e)||e<1||e>kt?null:e}` with `kt=1e4`.
const PAGE_MAX: u32 = 10_000;
/// Bundle: `listAllBacktests` always pages with `listBacktestsPage(e,s,100,n,r)`.
const PAGE_ALL_SIZE: u32 = 100;
/// Bundle: `var yt=1,bt=300` — accepted estimated signals per day.
const MIN_SIGNALS_PER_DAY: f64 = 1.0;
const MAX_SIGNALS_PER_DAY: f64 = 300.0;
/// Bundle: `for(let a=0;a<30;a++){...a<29&&await ml(2e3)}` in `estimateSignals`.
const ESTIMATE_ATTEMPTS: u32 = 30;
const ESTIMATE_INTERVAL: Duration = Duration::from_secs(2);
/// Bundle: `let i=["4H","6H","12H","24H","OFF"]`.
const SCHEDULE_INTERVALS: [&str; 5] = ["4H", "6H", "12H", "24H", "OFF"];

#[derive(clap::Subcommand, Debug)]
pub enum SignalCommand {
    /// Query custom signal feed (all sources by default)
    List(ListArgs),
    /// Manage custom signal strategies (create / update / delete / follow)
    Strategy {
        #[command(subcommand)]
        command: StrategyCommand,
    },
    /// Backtest management (list / detail / retry / schedule)
    Backtest {
        #[command(subcommand)]
        command: BacktestCommand,
    },
    /// Explore official signal strategies
    Explore(ExploreArgs),
    /// Query backtest credits info
    Credits,
    /// List wallet groups (for fomo-call --wallet-group-id)
    WalletGroup(WalletGroupArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum StrategyCommand {
    /// Create a new signal strategy (estimate → confirm → create)
    Create(StrategyCreateArgs),
    /// Update an existing strategy name or config
    Update(StrategyUpdateArgs),
    /// Delete a strategy (backtest task)
    Delete(StrategyDeleteArgs),
    /// Follow a strategy. Hall strategies are copied to your own account first.
    Follow(StrategyFollowArgs),
    /// Unfollow a strategy
    Unfollow(StrategyUnfollowArgs),
    /// List all your strategies (owned), optionally filtered by --followed or --type
    List(StrategyListArgs),
    /// List strategies you follow
    ListFollowed(StrategyListFollowedArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum BacktestCommand {
    /// List backtest task stats
    List(BacktestListArgs),
    /// Show backtest detail (task info + token list)
    Detail(BacktestDetailArgs),
    /// Retry a failed backtest job (meme-rush only)
    Retry(BacktestRetryArgs),
    /// Configure or query backtest schedule
    Schedule(BacktestScheduleArgs),
}

#[derive(clap::Args, Debug)]
pub struct ListArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId", default_value = "56")]
    pub chain_id: String,
    /// Number of signals per source
    #[arg(short = 'n', long, value_name = "size", default_value = "100")]
    pub page_size: String,
    /// Signal source: all | user | meme | smart-money
    #[arg(short = 's', long, value_name = "source", default_value = "all")]
    pub source: String,
    /// Filter by strategy ID (USER_STRATEGY only)
    #[arg(long, value_name = "id")]
    pub strategy_id: Option<String>,
    /// Filter by strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(long, value_name = "type")]
    pub strategy_type: Option<String>,
    /// Sort: time | maxGain
    #[arg(long, value_name = "field", default_value = "time")]
    pub sort_by: String,
    /// Time filter: 5m | 1h | 24h
    #[arg(long, value_name = "range")]
    pub time_range: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct StrategyCreateArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(short = 't', long = "type", value_name = "type")]
    pub strategy_type: String,
    /// Strategy name (max 20 chars)
    #[arg(short = 'n', long = "name", value_name = "name")]
    pub name: String,
    /// Strategy config (JSON string)
    #[arg(long, value_name = "config")]
    pub config: String,
    /// Wallet group ID (required for fomo-call, merged into config.selectedGroups)
    #[arg(long, value_name = "groupId")]
    pub wallet_group_id: Option<String>,
    /// Trigger backtest on creation (sets config.backtest.enabled=true)
    #[arg(long)]
    pub run_backtest: bool,
}

#[derive(clap::Args, Debug)]
pub struct StrategyUpdateArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(short = 't', long = "type", value_name = "type")]
    pub strategy_type: String,
    /// Job ID
    #[arg(long, value_name = "jobId")]
    pub job_id: String,
    /// New strategy name
    #[arg(short = 'n', long = "name", value_name = "name")]
    pub name: Option<String>,
    /// New strategy config (JSON string)
    #[arg(long, value_name = "config")]
    pub config: Option<String>,
    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct StrategyDeleteArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(short = 't', long = "type", value_name = "type")]
    pub strategy_type: String,
    /// Job ID
    #[arg(long, value_name = "jobId")]
    pub job_id: String,
    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct StrategyFollowArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(short = 't', long = "type", value_name = "type")]
    pub strategy_type: String,
    /// Job ID
    #[arg(long, value_name = "jobId")]
    pub job_id: String,
    /// Task ID (used to look up the strategy; hall strategies default to 1)
    #[arg(long, value_name = "taskId", default_value = "1")]
    pub task_id: String,
    /// Name for the copied strategy (only used when copying a hall strategy; max 20 chars)
    #[arg(short = 'n', long = "name", value_name = "name")]
    pub name: Option<String>,
    /// Skip the copy confirmation prompt for hall strategies
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct StrategyUnfollowArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type: meme-rush | fomo-call (aliases: meme, fomo)
    #[arg(short = 't', long = "type", value_name = "type")]
    pub strategy_type: String,
    /// Strategy ID
    #[arg(long, value_name = "strategyId")]
    pub strategy_id: String,
    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct StrategyListArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Only show followed strategies
    #[arg(long)]
    pub followed: bool,
    /// Filter by strategy type: meme-rush | fomo-call
    #[arg(long = "type", value_name = "strategyType")]
    pub strategy_type: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct StrategyListFollowedArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
}

#[derive(clap::Args, Debug)]
pub struct BacktestListArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Page number (ignored when --all is used)
    #[arg(short = 'p', long, value_name = "page", default_value = "1")]
    pub page: String,
    /// Page size
    #[arg(short = 's', long, value_name = "size", default_value = "20")]
    pub size: String,
    /// Backtest period in days
    #[arg(long, value_name = "days")]
    pub backtest_days: Option<String>,
    /// Fetch all pages automatically (overrides --page/--size)
    #[arg(long)]
    pub all: bool,
}

#[derive(clap::Args, Debug)]
pub struct BacktestDetailArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy ID
    #[arg(long, value_name = "strategyId")]
    pub strategy_id: String,
}

#[derive(clap::Args, Debug)]
pub struct BacktestRetryArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Strategy type (default: meme-rush)
    #[arg(
        short = 't',
        long = "type",
        value_name = "type",
        default_value = "meme-rush"
    )]
    pub strategy_type: String,
    /// Job ID
    #[arg(long, value_name = "jobId")]
    pub job_id: String,
}

#[derive(clap::Args, Debug)]
pub struct BacktestScheduleArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Job ID
    #[arg(long, value_name = "jobId")]
    pub job_id: String,
    /// Schedule interval: 4H | 6H | 12H | 24H | OFF (omit to query only)
    #[arg(long, value_name = "interval")]
    pub interval: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct ExploreArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
    /// Backtest period in days
    #[arg(long, value_name = "days")]
    pub backtest_days: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct WalletGroupArgs {
    /// Chain ID
    #[arg(short = 'c', long, value_name = "chainId")]
    pub chain_id: String,
}

pub fn is_mutation(command: &SignalCommand) -> bool {
    matches!(
        command,
        SignalCommand::Strategy {
            command: StrategyCommand::Create(_)
                | StrategyCommand::Update(_)
                | StrategyCommand::Delete(_)
                | StrategyCommand::Follow(_)
                | StrategyCommand::Unfollow(_)
        } | SignalCommand::Backtest {
            command: BacktestCommand::Retry(_) | BacktestCommand::Schedule(_)
        }
    )
}

/// Intent digest input: identifiers and strategy definitions only. Nothing here needs the
/// network, so the journal record is written before the first request and never contains
/// credentials, cookies, signatures or the `--yes` convenience flag.
pub fn mutation_payload(command: &SignalCommand) -> Result<Value, String> {
    match command {
        SignalCommand::Strategy { command } => match command {
            StrategyCommand::Create(args) => {
                let kind = strategy_type(&args.strategy_type)?;
                strategy_name(&args.name)?;
                let mut out = Map::new();
                out.insert("action".into(), json!("strategy.create"));
                out.insert("chainId".into(), json!(chain_id(&args.chain_id)?));
                out.insert("type".into(), json!(kind));
                out.insert("strategyName".into(), json!(args.name));
                out.insert("config".into(), create_config(args, kind)?);
                Ok(Value::Object(out))
            }
            StrategyCommand::Update(args) => {
                let (name, config) = update_inputs(args)?;
                let mut out = Map::new();
                out.insert("action".into(), json!("strategy.update"));
                out.insert("chainId".into(), json!(chain_id(&args.chain_id)?));
                out.insert("type".into(), json!(strategy_type(&args.strategy_type)?));
                out.insert(
                    "jobId".into(),
                    json!(identifier(&args.job_id, "invalid --job-id")?),
                );
                if let Some(name) = name {
                    out.insert("strategyName".into(), json!(name));
                }
                if let Some(config) = config {
                    out.insert("config".into(), config);
                }
                Ok(Value::Object(out))
            }
            StrategyCommand::Delete(args) => Ok(json!({
                "action": "strategy.delete",
                "chainId": chain_id(&args.chain_id)?,
                "type": strategy_type(&args.strategy_type)?,
                "jobId": identifier(&args.job_id, "invalid --job-id")?,
            })),
            StrategyCommand::Follow(args) => {
                let mut out = Map::new();
                out.insert("action".into(), json!("strategy.follow"));
                out.insert("chainId".into(), json!(chain_id(&args.chain_id)?));
                out.insert("type".into(), json!(strategy_type(&args.strategy_type)?));
                out.insert(
                    "jobId".into(),
                    json!(identifier(&args.job_id, "invalid --job-id")?),
                );
                out.insert(
                    "taskId".into(),
                    json!(identifier(&args.task_id, "invalid --task-id")?),
                );
                if let Some(name) = args.name.as_deref().filter(|name| !name.is_empty()) {
                    out.insert("strategyName".into(), json!(truncate_name(name)));
                }
                Ok(Value::Object(out))
            }
            StrategyCommand::Unfollow(args) => Ok(json!({
                "action": "strategy.unfollow",
                "chainId": chain_id(&args.chain_id)?,
                "type": strategy_type(&args.strategy_type)?,
                "strategyId": identifier(&args.strategy_id, "invalid --strategy-id")?,
            })),
            StrategyCommand::List(_) | StrategyCommand::ListFollowed(_) => {
                Err("signal: read-only command has no mutation payload".into())
            }
        },
        SignalCommand::Backtest { command } => match command {
            BacktestCommand::Retry(args) => Ok(json!({
                "action": "backtest.retry",
                "chainId": chain_id(&args.chain_id)?,
                "type": strategy_type(&args.strategy_type)?,
                "jobId": identifier(&args.job_id, "invalid --job-id")?,
            })),
            BacktestCommand::Schedule(args) => {
                let mut out = Map::new();
                out.insert("action".into(), json!("backtest.schedule"));
                out.insert("chainId".into(), json!(chain_id(&args.chain_id)?));
                out.insert(
                    "jobId".into(),
                    json!(identifier(&args.job_id, "invalid --job-id")?),
                );
                if let Some(interval) = args.interval.as_deref().filter(|value| !value.is_empty()) {
                    out.insert("interval".into(), json!(schedule_interval(interval)?));
                }
                Ok(Value::Object(out))
            }
            BacktestCommand::List(_) | BacktestCommand::Detail(_) => {
                Err("signal: read-only command has no mutation payload".into())
            }
        },
        SignalCommand::List(_)
        | SignalCommand::Explore(_)
        | SignalCommand::Credits
        | SignalCommand::WalletGroup(_) => {
            Err("signal: read-only command has no mutation payload".into())
        }
    }
}

pub async fn run(client: &mut Client, command: &SignalCommand) -> Result<Value, String> {
    match command {
        SignalCommand::List(args) => list(client, args).await,
        SignalCommand::Strategy { command } => match command {
            StrategyCommand::Create(args) => create(client, args).await,
            StrategyCommand::Update(args) => update(client, args).await,
            StrategyCommand::Delete(args) => delete(client, args).await,
            StrategyCommand::Follow(args) => follow(client, args).await,
            StrategyCommand::Unfollow(args) => unfollow(client, args).await,
            StrategyCommand::List(args) => list_strategies(client, args).await,
            StrategyCommand::ListFollowed(args) => list_followed(client, args).await,
        },
        SignalCommand::Backtest { command } => match command {
            BacktestCommand::List(args) => backtest_list(client, args).await,
            BacktestCommand::Detail(args) => backtest_detail(client, args).await,
            BacktestCommand::Retry(args) => backtest_retry(client, args).await,
            BacktestCommand::Schedule(args) => backtest_schedule(client, args).await,
        },
        SignalCommand::Explore(args) => explore(client, args).await,
        SignalCommand::Credits => credits(client).await,
        SignalCommand::WalletGroup(args) => wallet_group(client, args).await,
    }
}

/// `--source` accepts exactly the four documented spellings.
///
/// Bundle: `let i={all:"all",user:"USER_STRATEGY",meme:"MEME_OFFICIAL","smart-money":"SMART_MONEY"}[e.source]`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    All,
    User,
    Meme,
    SmartMoney,
}

impl Selection {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "all" => Ok(Self::All),
            "user" => Ok(Self::User),
            "meme" => Ok(Self::Meme),
            "smart-money" => Ok(Self::SmartMoney),
            _ => Err("invalid --source; expected all, user, meme, smart-money".into()),
        }
    }

    /// `Some` for a single-source selection, `None` when all three are queried.
    fn single(self) -> Option<Source> {
        match self {
            Self::All => None,
            Self::User => Some(Source::User),
            Self::Meme => Some(Source::Meme),
            Self::SmartMoney => Some(Source::SmartMoney),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    User,
    Meme,
    SmartMoney,
}

impl Source {
    /// Bundle: `signalSource:"USER_STRATEGY"`, `"MEME_OFFICIAL"`, `"SMART_MONEY"`.
    fn label(self) -> &'static str {
        match self {
            Self::User => "USER_STRATEGY",
            Self::Meme => "MEME_OFFICIAL",
            Self::SmartMoney => "SMART_MONEY",
        }
    }

    fn all() -> [Self; 3] {
        [Self::User, Self::Meme, Self::SmartMoney]
    }
}

/// Bundle: `let l=["time","maxGain"]`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Time,
    MaxGain,
}

impl Sort {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "time" => Ok(Self::Time),
            "maxGain" => Ok(Self::MaxGain),
            _ => Err("invalid --sort-by; expected time or maxGain".into()),
        }
    }
}

async fn list(client: &mut Client, args: &ListArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let size = page_size(&args.page_size)?;
    let selection = Selection::parse(&args.source)?;
    let sort = Sort::parse(&args.sort_by)?;
    let cutoff = time_cutoff(args.time_range.as_deref())?;
    let kind = args
        .strategy_type
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(strategy_type)
        .transpose()?;
    let strategy_id = args
        .strategy_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| identifier(value, "invalid --strategy-id"))
        .transpose()?;

    // Bundle: `e.strategyId&&i==="all"&&(i="USER_STRATEGY")` — a strategy filter always
    // narrows the feed to the user source, which is also why `getAllSignals` never has to
    // forward `strategyId`.
    let selection = match (selection, strategy_id) {
        (Selection::All, Some(_)) => Selection::User,
        (selection, _) => selection,
    };

    let (mut rows, failed) = match selection.single() {
        Some(source) => (
            tag_rows(
                fetch_source(client, chain, size, source, strategy_id, kind).await?,
                source,
            ),
            Vec::new(),
        ),
        None => fan_out(client, chain, size, kind).await?,
    };

    // Bundle: `o&&(u=u.filter(h=>h.signalSource!=="USER_STRATEGY"?!0:h.strategyType===o))`.
    if let Some(kind) = kind {
        rows.retain(|row| {
            row["signalSource"] != "USER_STRATEGY" || row["strategyType"].as_str() == Some(kind)
        });
    }
    if let Some(cutoff) = cutoff {
        // Bundle: `c!==null&&(y=y.filter(k=>k.signalTriggerTime>=c))`.
        rows.retain(|row| trigger_time(row) >= cutoff);
    }
    if sort == Sort::MaxGain {
        // Bundle: `e.sortBy==="maxGain"&&(y=[...y].sort((k,C)=>C.maxGain-k.maxGain))`.
        rows.sort_by(|left, right| max_gain(right).total_cmp(&max_gain(left)));
    }

    if selection == Selection::All {
        // Bundle JSON path: `g({list:y,allSucceeded:h.allSucceeded,failedSources:h.failedSources})`.
        Ok(json!({
            "list": rows,
            "allSucceeded": failed.is_empty(),
            "failedSources": failed
                .iter()
                .map(|source| json!({"source": source, "reason": FAILED_SOURCE_REASON}))
                .collect::<Vec<_>>(),
        }))
    } else {
        // Bundle JSON path: `g(y)`.
        Ok(Value::Array(rows))
    }
}

/// Backend error text is never echoed, so a failed source carries a fixed reason.
const FAILED_SOURCE_REASON: &str = "request failed";

async fn fan_out(
    client: &mut Client,
    chain: &str,
    size: u32,
    kind: Option<&str>,
) -> Result<(Vec<Value>, Vec<&'static str>), String> {
    // Bundle: `Promise.allSettled([query({chainId,pageSize,...strategyType?}),
    // queryMeme({chainId,pageSize}), querySmartMoney({chainId,pageSize,page:1})])` — a
    // rejected source is listed in `failedSources` and the rest are still returned.
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for source in Source::all() {
        match fetch_source(client, chain, size, source, None, kind).await {
            Ok(data) => rows.extend(tag_rows(data, source)),
            // A dead session must not be reported as "one source failed": the client has
            // already invalidated itself and every further call will fail too.
            Err(error) if error.contains("invalidated") => return Err(error),
            Err(_) => failed.push(source.label()),
        }
    }
    if failed.len() == Source::all().len() {
        // Bundle: `throw new m(d.SIGNAL_QUERY_ERROR, "All signal sources failed …")` with
        // message "Signal query failed".
        return Err("signal query failed".into());
    }
    // Bundle: `let u=l.sort((h,y)=>y.signalTriggerTime-h.signalTriggerTime)`.
    rows.sort_by(|left, right| trigger_time(right).total_cmp(&trigger_time(left)));
    Ok((rows, failed))
}

async fn fetch_source(
    client: &mut Client,
    chain: &str,
    size: u32,
    source: Source,
    strategy_id: Option<&str>,
    kind: Option<&str>,
) -> Result<Value, String> {
    let (path, body) = match source {
        // Bundle: `async query(e){return this.request(`${t.BASE_PATH}/query`,{method:"POST",body:e})}`
        // called as `this.api.query({chainId:e,pageSize:r,...o})` where `o` carries
        // `strategyId` and `strategyType` when set.
        Source::User => {
            let mut body = Map::new();
            body.insert("chainId".into(), json!(chain));
            body.insert("pageSize".into(), json!(size));
            if let Some(strategy_id) = strategy_id {
                body.insert("strategyId".into(), json!(strategy_id));
            }
            if let Some(kind) = kind {
                body.insert("strategyType".into(), json!(kind));
            }
            (BASE_PATH.to_owned() + "/query", Value::Object(body))
        }
        // Bundle: `async queryMeme(e){return this.request(t.MEME_PATH,{method:"POST",body:e})}`
        // with `{chainId,pageSize}`; the source=meme branch destructures `strategyType` out
        // (`let{strategyType:i,...a}=o??{}`) so it is never sent here.
        Source::Meme => (
            MEME_PATH.to_owned(),
            json!({"chainId": chain, "pageSize": size}),
        ),
        // Bundle: `async querySmartMoney(e){return this.request(t.SM_PATH,{method:"POST",body:e})}`
        // with `{chainId,pageSize,page:1}`.
        Source::SmartMoney => (
            SMART_MONEY_PATH.to_owned(),
            json!({"chainId": chain, "pageSize": size, "page": 1}),
        ),
    };
    client.request(Method::POST, &path, Some(body)).await
}

/// Bundle: `(s.value.data??[]).map(h=>({...h,signalSource:"…"}))`.
fn tag_rows(data: Value, source: Source) -> Vec<Value> {
    data.as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| match row {
            Value::Object(mut map) => {
                map.insert("signalSource".into(), json!(source.label()));
                Value::Object(map)
            }
            other => other,
        })
        .collect()
}

async fn create(client: &mut Client, args: &StrategyCreateArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    strategy_name(&args.name)?;
    let config = create_config(args, kind)?;

    // Bundle: `if(s!=="fomo-call"){let c=await i.estimateSignals(...);…checkBacktestLimit…}` —
    // fomo-call strategies skip both pre-checks entirely.
    if kind != "fomo-call" {
        let per_day = estimate(client, chain, kind, &config).await?;
        check_signal_rate(per_day, "create")?;
        check_backtest_limit(client, chain, kind, &config).await?;
    }

    // Bundle: `async createStrategy(e){return this.request(`${t.BASE_PATH}/config/create`,
    // {method:"POST",body:e})}` with `{type:s,chainId:n.chainId,strategyName:n.name,config:a}`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/config/create"),
            Some(
                json!({"type": kind, "chainId": chain, "strategyName": args.name, "config": config}),
            ),
        )
        .await?;
    // Bundle: `return{jobId:n.data.jobId,strategyId:n.data.strategyId}` — an absent field
    // disappears from the JSON object rather than becoming null.
    let mut out = Map::new();
    for key in ["jobId", "strategyId"] {
        if let Some(value) = data.get(key).filter(|value| !value.is_null()) {
            out.insert(key.into(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

async fn update(client: &mut Client, args: &StrategyUpdateArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    let job = identifier(&args.job_id, "invalid --job-id")?;
    let (name, config) = update_inputs(args)?;

    // Bundle: `await br(i,n.chainId,n.jobId,n.yes,"update this strategy")` — the copy-trade
    // warning defaults to "no", so it is a hard gate without --yes.
    copy_trade_guard(client, chain, job, args.yes).await?;

    if let Some(config) = config {
        if kind != "fomo-call" {
            let per_day = estimate(client, chain, kind, &config).await?;
            check_signal_rate(per_day, "update")?;
        }
        let mut body = Map::new();
        body.insert("chainId".into(), json!(chain));
        body.insert("type".into(), json!(kind));
        body.insert("jobId".into(), json!(job));
        body.insert("config".into(), config);
        if let Some(name) = name {
            body.insert("strategyName".into(), json!(name));
        }
        // Bundle: `async updateStrategy(e){return this.request(`${t.BASE_PATH}/strategy/update-single`,
        // {method:"POST",body:e})}` with `{chainId,type,jobId,config,...s?{strategyName:s}:{}}`.
        client
            .request(
                Method::POST,
                &format!("{BASE_PATH}/strategy/update-single"),
                Some(Value::Object(body)),
            )
            .await?;
    } else if let Some(name) = name {
        // Bundle: `async updateJobName(e,n,r){return this.request(`${t.BASE_PATH}/strategy/update-job-name`,
        // {method:"POST",body:{chainId:e,jobId:n,jobName:r}})}`.
        client
            .request(
                Method::POST,
                &format!("{BASE_PATH}/strategy/update-job-name"),
                Some(json!({"chainId": chain, "jobId": job, "jobName": name})),
            )
            .await?;
    }
    // Bundle JSON path: `g({success:!0})`; the backend body is not printed.
    Ok(json!({"success": true}))
}

async fn delete(client: &mut Client, args: &StrategyDeleteArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    let job = identifier(&args.job_id, "invalid --job-id")?;
    copy_trade_guard(client, chain, job, args.yes).await?;
    // Bundle: `if(!await Ke("This will delete the strategy and all associated backtests.
    // Continue?",n.yes)){console.log("Operation cancelled.");return}` — `[y/N]` defaults to no.
    if !args.yes {
        return Err("strategy delete requires --yes".into());
    }
    // Bundle: `async deleteJob(e,n,r){return this.request(`${t.BASE_PATH}/backtest/delete-job`,
    // {method:"POST",body:{chainId:e,type:n,jobIds:[r]}})}` and `.data.successJobIds.includes(r)`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/backtest/delete-job"),
            Some(json!({"chainId": chain, "type": kind, "jobIds": [job]})),
        )
        .await?;
    // Bundle JSON path: `g({success:l})`.
    Ok(json!({"success": includes_id(&data["successJobIds"], job)}))
}

async fn follow(client: &mut Client, args: &StrategyFollowArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    let job = identifier(&args.job_id, "invalid --job-id")?;
    let task = identifier(&args.task_id, "invalid --task-id")?;
    // Bundle: `async getTaskInfo(e,n,r,o){return this.request(`${t.BASE_PATH}/backtest/get-task-info`,
    // {method:"POST",body:{chainId:e,type:n,jobId:r,taskId:o}})}` and
    // `s.data||T(d.SIGNAL_STRATEGY_NOT_FOUND,…)`.
    let info = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/backtest/get-task-info"),
            Some(json!({"chainId": chain, "type": kind, "jobId": job, "taskId": task})),
        )
        .await?;
    if info.is_null() {
        return Err("strategy not found".into());
    }

    // Bundle: `let l=a.isOwner===!1` — importing a strategy-hall strategy is a two-step
    // copy-then-follow; only the copy step needs the backend config.
    let copied = info["isOwner"].as_bool() == Some(false);
    let mut target = job.to_owned();
    if copied {
        let config = info["taskParams"]["config"].clone();
        if config.is_null() {
            // Bundle: `a.taskParams?.config||T(d.SIGNAL_STRATEGY_NOT_FOUND,"Cannot copy hall strategy: config not available …")`.
            return Err("cannot copy hall strategy: config is not available".into());
        }
        // Bundle: `let h=a.hallNameCn||a.hallNameEn||a.taskName||"strategy"`.
        let hall = ["hallNameCn", "hallNameEn", "taskName"]
            .iter()
            .filter_map(|key| info[*key].as_str())
            .find(|name| !name.is_empty())
            .unwrap_or("strategy");
        // Bundle: `if(!await Ke(`Copy hall strategy "${h}" to your account and follow the
        // copy?`,n.yes))T(d.INVALID_PARAMS,"Operation cancelled by user")`.
        if !args.yes {
            return Err("copying a hall strategy requires --yes".into());
        }
        // Bundle: `let b=n.name||h;b.length>20&&(b=b.slice(0,20))`.
        let name = args
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(hall);
        let copy_name = truncate_name(name);
        // Bundle: `let A=a.taskParams.type??i`.
        let copy_kind = info["taskParams"]["type"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map_or_else(|| kind.to_owned(), str::to_owned);
        // Bundle: `let S={...a.taskParams.config},w=S.backtest??{};
        // w.time_range||(w.time_range="30d"),w.enabled||(w.enabled=!0),S.backtest=w`.
        let mut config = config;
        let backtest = config.as_object_mut().and_then(|object| {
            let missing = !object.get("backtest").is_some_and(Value::is_object);
            if missing {
                object.insert("backtest".into(), json!({}));
            }
            object.get_mut("backtest").and_then(Value::as_object_mut)
        });
        let Some(backtest) = backtest else {
            return Err("cannot copy hall strategy: config is not available".into());
        };
        if !backtest.get("time_range").is_some_and(is_truthy) {
            backtest.insert("time_range".into(), json!("30d"));
        }
        if !backtest.get("enabled").is_some_and(is_truthy) {
            backtest.insert("enabled".into(), json!(true));
        }

        if copy_kind != "fomo-call" {
            let per_day = estimate(client, chain, &copy_kind, &config).await?;
            check_signal_rate(per_day, "copy")?;
            check_backtest_limit(client, chain, &copy_kind, &config).await?;
        }
        // Bundle: `N=await s.createStrategy({type:A,chainId:n.chainId,strategyName:b,config:S})`,
        // `c=N.jobId`.
        let created = client
            .request(
                Method::POST,
                &format!("{BASE_PATH}/config/create"),
                Some(json!({
                    "type": copy_kind,
                    "chainId": chain,
                    "strategyName": copy_name,
                    "config": config,
                })),
            )
            .await?;
        target = created["jobId"]
            .as_str()
            .ok_or("strategy copy failed")?
            .to_owned();
    }

    // Bundle: `async follow(e,n,r){return this.request(`${t.BASE_PATH}/strategy/follow`,
    // {method:"POST",body:{chainId:e,type:n,jobId:r}})}` and `.data.followed`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/strategy/follow"),
            Some(json!({"chainId": chain, "type": kind, "jobId": target})),
        )
        .await?;
    // Bundle JSON path: `g({success:u,copied:l,jobId:c})`.
    Ok(json!({"success": data["followed"].clone(), "copied": copied, "jobId": target}))
}

async fn unfollow(client: &mut Client, args: &StrategyUnfollowArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    let strategy = identifier(&args.strategy_id, "invalid --strategy-id")?;

    // Bundle: `try{let u=(await s.listMyFollows(n.chainId)).find(h=>h.strategy_id===n.strategyId);
    // u&&await br(...)}catch(c){if(…"cancelled by user")throw c}` — a failed lookup is
    // swallowed on purpose, only the copy-trade cancellation escapes.
    if let Ok(follows) = list_my_follows(client, chain).await {
        if let Some(job) = follows
            .iter()
            .find(|row| row["strategy_id"].as_str() == Some(strategy))
            .and_then(|row| row["job_id"].as_str())
        {
            copy_trade_guard(client, chain, job, args.yes).await?;
        }
    }
    // Bundle: `if(!await Ke("Unfollow this strategy? This will stop signal notifications.",
    // n.yes)){console.log("Operation cancelled.");return}`.
    if !args.yes {
        return Err("strategy unfollow requires --yes".into());
    }
    // Bundle: `async unfollow(e,n,r){return this.request(`${t.BASE_PATH}/strategy/unfollow`,
    // {method:"POST",body:{chainId:e,type:n,strategyId:r}})}`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/strategy/unfollow"),
            Some(json!({"chainId": chain, "type": kind, "strategyId": strategy})),
        )
        .await?;
    // Bundle JSON path: `g({success:l})` where `l` is the raw `.data`.
    Ok(json!({"success": is_truthy(&data)}))
}

async fn list_strategies(client: &mut Client, args: &StrategyListArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    // Bundle: `let o=r?[r]:["meme-rush","fomo-call"]` — the raw `--type` value is forwarded,
    // it is never normalized through `Le`.
    let kinds: Vec<&str> = match args
        .strategy_type
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        Some(kind) => vec![kind],
        None => vec!["meme-rush", "fomo-call"],
    };
    let mut out = Vec::new();
    for kind in kinds {
        // Bundle: `a=(await Promise.all(o.map(l=>this.listAllBacktests(e,void 0,l)))).flat().filter(l=>l.isOwner)`.
        for task in list_all_backtests(client, chain, None, Some(kind)).await? {
            if !is_truthy(&task["isOwner"]) {
                continue;
            }
            // Bundle: `n&&(a=a.filter(l=>l.followed))`.
            if args.followed && !is_truthy(&task["followed"]) {
                continue;
            }
            // Bundle: `a.map(l=>({strategy_id:l.strategyId,strategy_name:l.taskName,
            // strategy_type:l.strategyType??l.type??"",job_id:l.jobId,task_id:l.taskId,followed:l.followed}))`.
            let strategy_type = task
                .get("strategyType")
                .filter(|value| !value.is_null())
                .or_else(|| task.get("type"))
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| json!(""));
            out.push(json!({
                "strategy_id": task["strategyId"].clone(),
                "strategy_name": task["taskName"].clone(),
                "strategy_type": strategy_type,
                "job_id": task["jobId"].clone(),
                "task_id": task["taskId"].clone(),
                "followed": task["followed"].clone(),
            }));
        }
    }
    // Bundle JSON path: `g(i)`.
    Ok(Value::Array(out))
}

async fn list_followed(
    client: &mut Client,
    args: &StrategyListFollowedArgs,
) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    // Bundle JSON path: `g(i)` where `i=await new G().listMyFollows(n.chainId)`.
    Ok(Value::Array(list_my_follows(client, chain).await?))
}

async fn list_my_follows(client: &mut Client, chain: &str) -> Result<Vec<Value>, String> {
    // Bundle: `async listMyFollows(e){return this.request(`${t.BASE_PATH}/strategy/list-my-follows`,
    // {method:"POST",body:{chainId:e}})}` and `(await this.api.listMyFollows(e)).data??[]`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/strategy/list-my-follows"),
            Some(json!({"chainId": chain})),
        )
        .await?;
    Ok(data.as_array().cloned().unwrap_or_default())
}

async fn backtest_list(client: &mut Client, args: &BacktestListArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let days = backtest_days(args.backtest_days.as_deref())?;
    if args.all {
        // Bundle: `let i=await s.listAllBacktests(n.chainId,n.backtestDays?Number(n.backtestDays):void 0)`
        // and `g({total:i.length,taskList:i})`.
        let tasks = list_all_backtests(client, chain, days, None).await?;
        return Ok(json!({"total": tasks.len(), "taskList": tasks}));
    }
    let page = page_number(&args.page)?;
    let size = page_size(&args.size)?;
    let data = list_page(client, chain, page, size, days, None).await?;
    if data.is_null() {
        // Bundle: `(await this.api.listTaskStats(e,n,r,o,s)).data??{taskList:[],page:n,size:r,total:0,totalPages:0}`.
        return Ok(json!({
            "taskList": [],
            "page": page,
            "size": size,
            "total": 0,
            "totalPages": 0,
        }));
    }
    // Bundle JSON path: `g(l)`.
    Ok(data)
}

/// Bundle: `listAllBacktests(e,n,r){let s=1,i=[],a=1;for(;s<=a;){let l=await
/// this.listBacktestsPage(e,s,100,n,r);i.push(...l.taskList),a=l.totalPages,
/// s===1&&l.total===0&&break;s++}return i}`.
async fn list_all_backtests(
    client: &mut Client,
    chain: &str,
    days: Option<u32>,
    kind: Option<&str>,
) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let mut page = 1u32;
    loop {
        let data = list_page(client, chain, page, PAGE_ALL_SIZE, days, kind).await?;
        rows.extend(data["taskList"].as_array().cloned().unwrap_or_default());
        if page == 1 && data["total"].as_u64().unwrap_or(0) == 0 {
            break;
        }
        // `a=l.totalPages` stops the walk as soon as the value is absent; the same cap as
        // `--page` keeps a hostile `totalPages` from turning into an unbounded request loop.
        let total_pages = data["totalPages"]
            .as_u64()
            .unwrap_or(0)
            .min(PAGE_MAX.into());
        if u64::from(page) >= total_pages {
            break;
        }
        page += 1;
    }
    Ok(rows)
}

async fn list_page(
    client: &mut Client,
    chain: &str,
    page: u32,
    size: u32,
    days: Option<u32>,
    kind: Option<&str>,
) -> Result<Value, String> {
    let mut body = Map::new();
    body.insert("chainId".into(), json!(chain));
    body.insert("page".into(), json!(page));
    body.insert("size".into(), json!(size));
    if let Some(days) = days {
        body.insert("backtestDays".into(), json!(days));
    }
    if let Some(kind) = kind {
        body.insert("strategyType".into(), json!(kind));
    }
    // Bundle: `async listTaskStats(e,n,r,o,s){return this.request(`${t.BASE_PATH}/backtest/list-task-stats`,
    // {method:"POST",body:{chainId:e,page:n,size:r,...o?{backtestDays:o}:{},...s?{strategyType:s}:{}}})}`.
    client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/backtest/list-task-stats"),
            Some(Value::Object(body)),
        )
        .await
}

async fn backtest_detail(client: &mut Client, args: &BacktestDetailArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let strategy = identifier(&args.strategy_id, "invalid --strategy-id")?;
    // Bundle: `async getTaskVo(e,n,r){return this.request(`${t.BASE_PATH}/backtest/get-task-vo`,
    // {method:"POST",body:{chainId:e,strategyId:n,...r?{backtestDays:r}:{}}})}`; `backtest
    // detail` passes no period.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/backtest/get-task-vo"),
            Some(json!({"chainId": chain, "strategyId": strategy})),
        )
        .await?;
    if data.is_null() {
        // Bundle: `r.data||T(d.SIGNAL_STRATEGY_NOT_FOUND,`Strategy not found: strategyId=…`)`.
        return Err("strategy not found".into());
    }
    // Bundle: `{info:r.data,tokens:r.data.tokenDetails??[]}`.
    let tokens = data
        .get("tokenDetails")
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok(json!({"info": data, "tokens": tokens}))
}

async fn backtest_retry(client: &mut Client, args: &BacktestRetryArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let kind = strategy_type(&args.strategy_type)?;
    let job = identifier(&args.job_id, "invalid --job-id")?;
    // Bundle: `async retryJob(e,n,r){return this.request(`${t.BASE_PATH}/backtest/retry-job`,
    // {method:"POST",body:{chainId:e,type:n,jobIds:[r]}})}` and `.data.successJobIds.includes(r)`.
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/backtest/retry-job"),
            Some(json!({"chainId": chain, "type": kind, "jobIds": [job]})),
        )
        .await?;
    // Bundle JSON path: `g({success:a})`.
    Ok(json!({"success": includes_id(&data["successJobIds"], job)}))
}

async fn backtest_schedule(
    client: &mut Client,
    args: &BacktestScheduleArgs,
) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let job = identifier(&args.job_id, "invalid --job-id")?;
    if let Some(interval) = args.interval.as_deref().filter(|value| !value.is_empty()) {
        let interval = schedule_interval(interval)?;
        // Bundle: `async configSchedule(e,n,r){return this.request(`${t.BASE_PATH}/schedule/config`,
        // {method:"POST",body:{chainId:e,jobId:n,scheduleInterval:r}})}`.
        return client
            .request(
                Method::POST,
                &format!("{BASE_PATH}/schedule/config"),
                Some(json!({"chainId": chain, "jobId": job, "scheduleInterval": interval})),
            )
            .await;
    }
    // Bundle: `else{let i=await s.querySchedule(n.chainId,n.jobId)...}` with
    // `async querySchedule(e,n){return this.request(`${t.BASE_PATH}/schedule/query`,
    // {method:"POST",body:{chainId:e,jobId:n}})}`.
    client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/schedule/query"),
            Some(json!({"chainId": chain, "jobId": job})),
        )
        .await
}

async fn explore(client: &mut Client, args: &ExploreArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let days = backtest_days(args.backtest_days.as_deref())?;
    let mut body = Map::new();
    body.insert("chainId".into(), json!(chain));
    if let Some(days) = days {
        body.insert("backtestDays".into(), json!(days));
    }
    // Bundle: `async exploreOfficial(e,n){return this.request("/bapi/defi/v1/public/wallet-direct/
    // custom-signal/strategy/explore",{method:"POST",body:{chainId:e,...n?{backtestDays:n}:{}}})}`
    // and `(await this.api.exploreOfficial(e,n)).data?.taskList??[]`.
    let data = client
        .request(Method::POST, EXPLORE_PATH, Some(Value::Object(body)))
        .await?;
    Ok(data
        .get("taskList")
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([])))
}

async fn credits(client: &mut Client) -> Result<Value, String> {
    // Bundle: `async creditsInfo(){return this.request(`${t.BASE_PATH}/credits/info`,
    // {method:"POST",body:{}})}` and `(await this.api.creditsInfo()).data`.
    client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/credits/info"),
            Some(json!({})),
        )
        .await
}

async fn wallet_group(client: &mut Client, args: &WalletGroupArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    // Bundle: `var xn=class extends E{async listGroups(e,n=!0){return this.request(
    // "/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet/group/list",
    // {method:"POST",body:{chainId:e,includeAllGroup:n}})}}`.
    client
        .request(
            Method::POST,
            WALLET_GROUP_PATH,
            Some(json!({"chainId": chain, "includeAllGroup": true})),
        )
        .await
}

/// Bundle: `async br(t,e,n,r,o){…let i=(await t.listAllBacktests(e)).find(a=>a.jobId===n);
/// i?.copyTradeStatus==="ACTIVE"&&(…await Ke(`…Continue?`,r)||T(d.INVALID_PARAMS,"Operation
/// cancelled by user"))}catch{…}` — a failed lookup is swallowed, a refused warning aborts.
async fn copy_trade_guard(
    client: &mut Client,
    chain: &str,
    job: &str,
    yes: bool,
) -> Result<(), String> {
    let Ok(tasks) = list_all_backtests(client, chain, None, None).await else {
        return Ok(());
    };
    let active = tasks
        .iter()
        .find(|task| task["jobId"].as_str() == Some(job))
        .is_some_and(|task| task["copyTradeStatus"].as_str() == Some("ACTIVE"));
    if active && !yes {
        return Err("strategy has active copy trading; rerun with --yes to continue".into());
    }
    Ok(())
}

/// Bundle: `async checkBacktestLimit(e,n,r){return this.request(`${t.BASE_PATH}/strategy/check-backtest-limit`,
/// {method:"POST",body:{chainId:e,type:n,config:r}})}`; `u.isValid||T(d.SIGNAL_DAILY_LIMIT,…)`.
async fn check_backtest_limit(
    client: &mut Client,
    chain: &str,
    kind: &str,
    config: &Value,
) -> Result<(), String> {
    let data = client
        .request(
            Method::POST,
            &format!("{BASE_PATH}/strategy/check-backtest-limit"),
            Some(json!({"chainId": chain, "type": kind, "config": config})),
        )
        .await?;
    if !is_truthy(&data["isValid"]) {
        return Err("daily backtest limit reached".into());
    }
    Ok(())
}

/// Bundle: `estimateSignals(e,n,r){let i="PENDING";for(let a=0;a<30;a++){let c=(await
/// this.api.countSignals({chainId:e,type:n,config:r})).data;if(i=c.status,i.status==="COMPLETED")
/// {let u=c.totalSignalCount??0,h=ul(r);return h>0?u/h:u}a<29&&await ml(2e3)}throw new Error(…)}`.
async fn estimate(
    client: &mut Client,
    chain: &str,
    kind: &str,
    config: &Value,
) -> Result<f64, String> {
    for attempt in 0..ESTIMATE_ATTEMPTS {
        // Bundle: `async countSignals(e){return this.request(`${t.BASE_PATH}/strategy/count-signals`,
        // {method:"POST",body:e})}`.
        let data = client
            .request(
                Method::POST,
                &format!("{BASE_PATH}/strategy/count-signals"),
                Some(json!({"chainId": chain, "type": kind, "config": config})),
            )
            .await?;
        if data["status"].as_str() == Some("COMPLETED") {
            let total = data["totalSignalCount"].as_f64().unwrap_or(0.0);
            let days = backtest_period_days(config);
            return Ok(if days > 0.0 { total / days } else { total });
        }
        if attempt + 1 < ESTIMATE_ATTEMPTS {
            tokio::time::sleep(ESTIMATE_INTERVAL).await;
        }
    }
    // Bundle: `throw new Error(`count-signals did not complete after 30 attempts (last status: …)`)`;
    // the status text is deliberately not echoed.
    Err("signal count estimate did not complete".into())
}

/// Bundle: `function ul(t){let n=t.backtest?.time_range;if(!n)return 30;
/// let r=n.match(/^(\d+)d$/i);return r?parseInt(r[1],10):30}`.
fn backtest_period_days(config: &Value) -> f64 {
    let Some(range) = config
        .get("backtest")
        .and_then(|backtest| backtest.get("time_range"))
        .and_then(Value::as_str)
    else {
        return 30.0;
    };
    let Some(digits) = range
        .strip_suffix('d')
        .or_else(|| range.strip_suffix('D'))
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
    else {
        return 30.0;
    };
    digits.parse::<f64>().unwrap_or(30.0)
}

/// Bundle: `c<yt&&T(d.INVALID_PARAMS,`Estimated ${c} signals/day — too few (< 1/day)…`)`
/// and the `bt=300` upper bound. The estimate itself is not echoed.
fn check_signal_rate(per_day: f64, action: &str) -> Result<(), String> {
    if per_day < MIN_SIGNALS_PER_DAY {
        return Err(format!(
            "estimated signals per day too few (< {MIN_SIGNALS_PER_DAY:.0}/day); cannot {action} strategy"
        ));
    }
    if per_day > MAX_SIGNALS_PER_DAY {
        return Err(format!(
            "estimated signals per day too many (> {MAX_SIGNALS_PER_DAY:.0}/day); cannot {action} strategy"
        ));
    }
    Ok(())
}

/// Bundle `ts(t,e)`: parse the JSON, then reject `{}` for every type except fomo-call.
fn parse_config(value: &str, kind: &str) -> Result<Value, String> {
    let parsed: Value = serde_json::from_str(value).map_err(|_| "invalid JSON in --config")?;
    if kind != "fomo-call" && parsed.as_object().is_some_and(Map::is_empty) {
        return Err("empty config is not valid for this strategy type".into());
    }
    Ok(parsed)
}

/// Bundle create body assembly: `a.selectedGroups={presetGroup:null,customGroupId:Number(…)}`
/// and `let c=a.backtest??{};c.enabled=!0;a.backtest=c`.
fn create_config(args: &StrategyCreateArgs, kind: &str) -> Result<Value, String> {
    let mut config = parse_config(&args.config, kind)?;
    if let Some(group) = args
        .wallet_group_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let object = config
            .as_object_mut()
            .ok_or("strategy config must be a JSON object")?;
        object.insert(
            "selectedGroups".into(),
            json!({"presetGroup": Value::Null, "customGroupId": group_number(group)}),
        );
    }
    if args.run_backtest {
        let object = config
            .as_object_mut()
            .ok_or("strategy config must be a JSON object")?;
        if !object.get("backtest").is_some_and(Value::is_object) {
            object.insert("backtest".into(), json!({}));
        }
        if let Some(Value::Object(backtest)) = object.get_mut("backtest") {
            backtest.insert("enabled".into(), json!(true));
        }
    }
    Ok(config)
}

/// Bundle: `Number(n.walletGroupId)`; anything numeric becomes a JSON number and a
/// non-numeric value serializes the way `JSON.stringify(NaN)` does, as `null`.
fn group_number(value: &str) -> Value {
    if let Ok(integer) = value.parse::<i64>() {
        return json!(integer);
    }
    match value.parse::<f64>() {
        Ok(number) if number.is_finite() => json!(number),
        _ => Value::Null,
    }
}

/// Shared by `run` and `mutation_payload` so the journal digest and the wire body agree.
/// Bundle order: the at-least-one check, the name length, `Le(n.type)`, then `ts(n.config,s)`.
fn update_inputs(args: &StrategyUpdateArgs) -> Result<(Option<&str>, Option<Value>), String> {
    let name = args.name.as_deref().filter(|value| !value.is_empty());
    if name.is_none() && args.config.is_none() {
        // Bundle: `!n.name&&!n.config&&T(d.INVALID_PARAMS,"At least --name or --config must be provided")`.
        return Err("at least --name or --config must be provided".into());
    }
    if let Some(name) = name {
        strategy_name(name)?;
    }
    let kind = strategy_type(&args.strategy_type)?;
    let config = args
        .config
        .as_deref()
        .map(|config| parse_config(config, kind))
        .transpose()?;
    Ok((name, config))
}

fn strategy_type(value: &str) -> Result<&'static str, String> {
    // Bundle: `var Xa={meme:"meme-rush",fomo:"fomo-call","meme-rush":"meme-rush","fomo-call":"fomo-call"}`.
    match value.trim().to_ascii_lowercase().as_str() {
        "meme" | "meme-rush" => Ok("meme-rush"),
        "fomo" | "fomo-call" => Ok("fomo-call"),
        _ => Err(
            "invalid strategy type; expected meme-rush or fomo-call (aliases: meme, fomo)".into(),
        ),
    }
}

/// Bundle: `n.name.length>20&&T(d.INVALID_PARAMS,"Strategy name must be ≤ 20 characters")`.
fn strategy_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("invalid --name".into());
    }
    if value.chars().count() > 20 {
        return Err("strategy name must be at most 20 characters".into());
    }
    Ok(())
}

/// Bundle: `b.length>20&&(b=b.slice(0,20))`.
fn truncate_name(value: &str) -> String {
    value.chars().take(20).collect()
}

fn schedule_interval(value: &str) -> Result<&str, String> {
    // Bundle: `let i=["4H","6H","12H","24H","OFF"];i.includes(n.interval)||T(d.INVALID_PARAMS,…)`.
    if SCHEDULE_INTERVALS.contains(&value) {
        Ok(value)
    } else {
        Err("invalid --interval; expected 4H, 6H, 12H, 24H or OFF".into())
    }
}

/// Bundle: `gl(t)` — `5m`, `1h` and `24h` are the only accepted windows.
fn time_cutoff(range: Option<&str>) -> Result<Option<f64>, String> {
    let Some(range) = range.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let delta = match range {
        "5m" => 300_000.0,
        "1h" => 3_600_000.0,
        "24h" => 86_400_000.0,
        _ => return Err("invalid --time-range; expected 5m, 1h, 24h".into()),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0);
    Ok(Some(now - delta))
}

fn page_size(value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "invalid page size; expected an integer between 1 and 100")?;
    if value.trim() != value || !(1..=PAGE_SIZE_MAX).contains(&parsed) {
        return Err("invalid page size; expected an integer between 1 and 100".into());
    }
    Ok(parsed)
}

fn page_number(value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "invalid page number; expected an integer between 1 and 10000")?;
    if value.trim() != value || !(1..=PAGE_MAX).contains(&parsed) {
        return Err("invalid page number; expected an integer between 1 and 10000".into());
    }
    Ok(parsed)
}

fn backtest_days(value: Option<&str>) -> Result<Option<u32>, String> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "invalid --backtest-days; expected a positive integer")?;
    if value.trim() != value || parsed == 0 {
        return Err("invalid --backtest-days; expected a positive integer".into());
    }
    Ok(Some(parsed))
}

fn chain_id(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("invalid --chain-id".into());
    }
    Ok(value)
}

fn identifier<'a>(value: &'a str, error: &str) -> Result<&'a str, String> {
    if value.trim().is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(error.to_owned());
    }
    Ok(value)
}

/// Bundle: `.data.successJobIds.includes(r)`.
fn includes_id(list: &Value, id: &str) -> bool {
    list.as_array()
        .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(id)))
}

/// JavaScript truthiness for backend fields (`isOwner`, `followed`, `isValid`, `.data`).
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

fn trigger_time(row: &Value) -> f64 {
    row["signalTriggerTime"].as_f64().unwrap_or(0.0)
}

fn max_gain(row: &Value) -> f64 {
    row["maxGain"].as_f64().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Credentials;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc::{channel, Receiver},
        thread::JoinHandle,
        time::Duration,
    };

    fn data(value: Value) -> Value {
        json!({"code": "000000", "data": value})
    }

    fn serve(replies: Vec<Value>) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let replies: Vec<String> = replies.into_iter().map(|value| value.to_string()).collect();
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || {
            for body in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0, "client closed before sending a full request");
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let len = String::from_utf8_lossy(&bytes[..end])
                            .to_lowercase()
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });
        (origin, rx, handle)
    }

    fn client(origin: &str) -> Client {
        Client::test_client(
            Credentials {
                client_id: zeroize::Zeroizing::new("synthetic-client".into()),
                session_id: zeroize::Zeroizing::new("synthetic-session".into()),
            },
            origin,
        )
        .unwrap()
    }

    fn request_line(request: &str) -> &str {
        request.lines().next().unwrap()
    }

    fn body_of(request: &str) -> Value {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    fn post(path: &str) -> String {
        format!("POST {path} HTTP/1.1")
    }

    fn list_args() -> ListArgs {
        ListArgs {
            chain_id: "56".into(),
            page_size: "100".into(),
            source: "all".into(),
            strategy_id: None,
            strategy_type: None,
            sort_by: "time".into(),
            time_range: None,
        }
    }

    fn create_args() -> StrategyCreateArgs {
        StrategyCreateArgs {
            chain_id: "56".into(),
            strategy_type: "meme".into(),
            name: "synthetic".into(),
            config: r#"{"filter":{"minHolders":10},"backtest":{"time_range":"30d"}}"#.into(),
            wallet_group_id: None,
            run_backtest: false,
        }
    }

    fn update_args() -> StrategyUpdateArgs {
        StrategyUpdateArgs {
            chain_id: "56".into(),
            strategy_type: "meme-rush".into(),
            job_id: "job-1".into(),
            name: None,
            config: None,
            yes: false,
        }
    }

    fn follow_args() -> StrategyFollowArgs {
        StrategyFollowArgs {
            chain_id: "56".into(),
            strategy_type: "meme-rush".into(),
            job_id: "job-1".into(),
            task_id: "1".into(),
            name: None,
            yes: false,
        }
    }

    fn task_row(job: &str) -> Value {
        json!({
            "strategyId":"strategy-1","taskName":"synthetic","strategyType":"meme-rush",
            "jobId":job,"taskId":"1","followed":true,"isOwner":true,
            "copyTradeStatus":"INACTIVE","totalPages":1,"total":1,
            "taskList":[{"strategyId":"strategy-1","taskName":"synthetic","jobId":job}]
        })
    }

    #[tokio::test]
    async fn list_all_sources_queries_each_endpoint_tags_and_sorts() {
        let (origin, rx, server) = serve(vec![
            data(json!([{"signalId":"u1","signalTriggerTime":3,"strategyType":"meme-rush"}])),
            data(json!([{"signalId":"m1","signalTriggerTime":5}])),
            data(json!([{"signalId":"s1","signalTriggerTime":4}])),
        ]);
        let mut client = client(&origin);

        let got = run(&mut client, &SignalCommand::List(list_args()))
            .await
            .unwrap();
        assert_eq!(got["allSucceeded"], true);
        assert_eq!(got["failedSources"], json!([]));
        let ids: Vec<&str> = got["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["signalId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["m1", "s1", "u1"]);
        assert_eq!(got["list"][2]["signalSource"], "USER_STRATEGY");
        assert_eq!(got["list"][0]["signalSource"], "MEME_OFFICIAL");
        assert_eq!(got["list"][1]["signalSource"], "SMART_MONEY");

        let first = rx.recv().unwrap();
        assert_eq!(request_line(&first), post(&format!("{BASE_PATH}/query")));
        assert_eq!(body_of(&first), json!({"chainId":"56","pageSize":100}));
        let second = rx.recv().unwrap();
        assert_eq!(request_line(&second), post(MEME_PATH));
        assert_eq!(body_of(&second), json!({"chainId":"56","pageSize":100}));
        let third = rx.recv().unwrap();
        assert_eq!(request_line(&third), post(SMART_MONEY_PATH));
        assert_eq!(
            body_of(&third),
            json!({"chainId":"56","pageSize":100,"page":1})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_user_source_forwards_filters_and_filters_by_strategy_type() {
        let raw = json!([
            {"signalId":"u1","signalTriggerTime":3,"strategyType":"meme-rush"},
            {"signalId":"u2","signalTriggerTime":4,"strategyType":"fomo-call"}
        ]);
        let (origin, rx, server) = serve(vec![data(raw)]);
        let mut client = client(&origin);
        let args = ListArgs {
            source: "user".into(),
            strategy_id: Some("strategy-1".into()),
            strategy_type: Some("meme".into()),
            ..list_args()
        };

        let got = run(&mut client, &SignalCommand::List(args)).await.unwrap();
        assert_eq!(
            got,
            json!([{"signalId":"u1","signalTriggerTime":3,"strategyType":"meme-rush","signalSource":"USER_STRATEGY"}])
        );
        let request = rx.recv().unwrap();
        assert_eq!(request_line(&request), post(&format!("{BASE_PATH}/query")));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","pageSize":100,"strategyId":"strategy-1","strategyType":"meme-rush"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_strategy_filter_narrows_all_to_the_user_source() {
        let (origin, rx, server) = serve(vec![data(json!([]))]);
        let mut client = client(&origin);
        let args = ListArgs {
            strategy_id: Some("strategy-9".into()),
            ..list_args()
        };

        assert_eq!(
            run(&mut client, &SignalCommand::List(args)).await.unwrap(),
            json!([])
        );
        let request = rx.recv().unwrap();
        assert_eq!(request_line(&request), post(&format!("{BASE_PATH}/query")));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","pageSize":100,"strategyId":"strategy-9"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_time_range_and_max_gain_sort_apply_after_fan_in() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let (origin, rx, server) = serve(vec![
            data(json!([
                {"signalId":"old","signalTriggerTime":1,"maxGain":9},
                {"signalId":"new","signalTriggerTime":now,"maxGain":1}
            ])),
            data(json!([{"signalId":"meme","signalTriggerTime":now,"maxGain":5}])),
            data(json!([])),
        ]);
        let mut client = client(&origin);
        let args = ListArgs {
            sort_by: "maxGain".into(),
            time_range: Some("24h".into()),
            ..list_args()
        };

        let got = run(&mut client, &SignalCommand::List(args)).await.unwrap();
        let ids: Vec<&str> = got["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["signalId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["meme", "new"]);
        for _ in 0..3 {
            rx.recv().unwrap();
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_reports_partial_failures_without_echoing_backend_text() {
        let (origin, rx, server) = serve(vec![
            json!({"code":"61001001","message":"secret-backend-text","data":null}),
            data(json!([{"signalId":"m1","signalTriggerTime":5}])),
            json!({"code":"61001001","message":"secret-backend-text","data":null}),
        ]);
        let mut client = client(&origin);

        let got = run(&mut client, &SignalCommand::List(list_args()))
            .await
            .unwrap();
        assert_eq!(got["allSucceeded"], false);
        assert_eq!(got["list"][0]["signalId"], "m1");
        let text = got.to_string();
        assert!(!text.contains("secret-backend-text"));
        assert!(!text.contains("61001001"));
        assert_eq!(
            got["failedSources"],
            json!([
                {"source":"USER_STRATEGY","reason":"request failed"},
                {"source":"SMART_MONEY","reason":"request failed"}
            ])
        );
        for _ in 0..3 {
            rx.recv().unwrap();
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn list_fails_when_every_source_fails() {
        let (origin, rx, server) = serve(vec![
            json!({"code":"61001001","message":"x","data":null}),
            json!({"code":"61001001","message":"x","data":null}),
            json!({"code":"61001001","message":"x","data":null}),
        ]);
        let mut client = client(&origin);

        assert_eq!(
            run(&mut client, &SignalCommand::List(list_args()))
                .await
                .unwrap_err(),
            "signal query failed"
        );
        for _ in 0..3 {
            rx.recv().unwrap();
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_create_estimates_checks_limit_then_posts_definition() {
        let (origin, rx, server) = serve(vec![
            data(json!({"status":"COMPLETED","totalSignalCount":60})),
            data(json!({"isValid":true,"backtestCount":1,"dailyLimit":5})),
            data(json!({"jobId":"job-9","strategyId":"strategy-9","ignored":true})),
        ]);
        let mut client = client(&origin);
        let command = SignalCommand::Strategy {
            command: StrategyCommand::Create(create_args()),
        };

        assert_eq!(
            run(&mut client, &command).await.unwrap(),
            json!({"jobId":"job-9","strategyId":"strategy-9"})
        );

        let count = rx.recv().unwrap();
        assert_eq!(
            request_line(&count),
            post(&format!("{BASE_PATH}/strategy/count-signals"))
        );
        assert_eq!(
            body_of(&count),
            json!({
                "chainId":"56","type":"meme-rush",
                "config":{"filter":{"minHolders":10},"backtest":{"time_range":"30d"}}
            })
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/strategy/check-backtest-limit"))
        );
        let created = rx.recv().unwrap();
        assert_eq!(
            request_line(&created),
            post(&format!("{BASE_PATH}/config/create"))
        );
        assert_eq!(
            body_of(&created),
            json!({
                "type":"meme-rush","chainId":"56","strategyName":"synthetic",
                "config":{"filter":{"minHolders":10},"backtest":{"time_range":"30d"}}
            })
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_create_merges_wallet_group_and_backtest_flag() {
        let (origin, rx, server) = serve(vec![data(json!({"jobId":"job-1","strategyId":"s1"}))]);
        let mut client = client(&origin);
        let args = StrategyCreateArgs {
            strategy_type: "fomo".into(),
            config: r#"{"filter":{"minHolders":10}}"#.into(),
            wallet_group_id: Some("42".into()),
            run_backtest: true,
            ..create_args()
        };

        run(
            &mut client,
            &SignalCommand::Strategy {
                command: StrategyCommand::Create(args),
            },
        )
        .await
        .unwrap();
        let created = rx.recv().unwrap();
        assert_eq!(
            body_of(&created),
            json!({
                "type":"fomo-call","chainId":"56","strategyName":"synthetic",
                "config":{
                    "filter":{"minHolders":10},
                    "selectedGroups":{"presetGroup":null,"customGroupId":42},
                    "backtest":{"enabled":true}
                }
            })
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_update_posts_config_and_name() {
        let (origin, rx, server) = serve(vec![
            data(task_row("job-1")),
            data(json!({"status":"COMPLETED","totalSignalCount":90})),
            data(json!({})),
        ]);
        let mut client = client(&origin);
        let args = StrategyUpdateArgs {
            name: Some("renamed".into()),
            config: Some(r#"{"filter":{"minHolders":10},"backtest":{"time_range":"30d"}}"#.into()),
            ..update_args()
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::Update(args),
                },
            )
            .await
            .unwrap(),
            json!({"success": true})
        );

        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/backtest/list-task-stats"))
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/strategy/count-signals"))
        );
        let update = rx.recv().unwrap();
        assert_eq!(
            request_line(&update),
            post(&format!("{BASE_PATH}/strategy/update-single"))
        );
        assert_eq!(
            body_of(&update),
            json!({
                "chainId":"56","type":"meme-rush","jobId":"job-1",
                "config":{"filter":{"minHolders":10},"backtest":{"time_range":"30d"}},
                "strategyName":"renamed"
            })
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_update_with_only_a_name_uses_update_job_name() {
        let (origin, rx, server) = serve(vec![data(task_row("job-1")), data(json!({"ok":true}))]);
        let mut client = client(&origin);
        let args = StrategyUpdateArgs {
            name: Some("renamed".into()),
            yes: true,
            ..update_args()
        };

        run(
            &mut client,
            &SignalCommand::Strategy {
                command: StrategyCommand::Update(args),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/backtest/list-task-stats"))
        );
        let update = rx.recv().unwrap();
        assert_eq!(
            request_line(&update),
            post(&format!("{BASE_PATH}/strategy/update-job-name"))
        );
        assert_eq!(
            body_of(&update),
            json!({"chainId":"56","jobId":"job-1","jobName":"renamed"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_delete_confirms_then_posts_job_ids() {
        let (origin, rx, server) = serve(vec![
            data(task_row("job-1")),
            data(json!({"successJobIds":["job-1"]})),
        ]);
        let mut client = client(&origin);
        let args = StrategyDeleteArgs {
            chain_id: "56".into(),
            strategy_type: "meme-rush".into(),
            job_id: "job-1".into(),
            yes: true,
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::Delete(args),
                },
            )
            .await
            .unwrap(),
            json!({"success": true})
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/backtest/list-task-stats"))
        );
        let delete = rx.recv().unwrap();
        assert_eq!(
            request_line(&delete),
            post(&format!("{BASE_PATH}/backtest/delete-job"))
        );
        assert_eq!(
            body_of(&delete),
            json!({"chainId":"56","type":"meme-rush","jobIds":["job-1"]})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_follow_follows_an_owned_strategy_directly() {
        let (origin, rx, server) = serve(vec![
            data(json!({"isOwner":true,"jobId":"job-1","taskParams":{"config":{}}})),
            data(json!({"followed":true})),
        ]);
        let mut client = client(&origin);

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::Follow(follow_args()),
                },
            )
            .await
            .unwrap(),
            json!({"success":true,"copied":false,"jobId":"job-1"})
        );
        let info = rx.recv().unwrap();
        assert_eq!(
            request_line(&info),
            post(&format!("{BASE_PATH}/backtest/get-task-info"))
        );
        assert_eq!(
            body_of(&info),
            json!({"chainId":"56","type":"meme-rush","jobId":"job-1","taskId":"1"})
        );
        let follow = rx.recv().unwrap();
        assert_eq!(
            request_line(&follow),
            post(&format!("{BASE_PATH}/strategy/follow"))
        );
        assert_eq!(
            body_of(&follow),
            json!({"chainId":"56","type":"meme-rush","jobId":"job-1"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_follow_copies_a_hall_strategy_before_following() {
        let (origin, rx, server) = serve(vec![
            data(json!({
                "isOwner":false,"jobId":"job-1","taskId":"1",
                "hallNameEn":"hall-copy","taskName":"fallback",
                "taskParams":{"type":"meme-rush","config":{"filter":{"minHolders":10}}}
            })),
            data(json!({"status":"COMPLETED","totalSignalCount":120})),
            data(json!({"isValid":true})),
            data(json!({"jobId":"job-2","strategyId":"strategy-2"})),
            data(json!({"followed":true})),
        ]);
        let mut client = client(&origin);
        let args = StrategyFollowArgs {
            name: Some("a-very-long-strategy-name-for-truncation".into()),
            yes: true,
            ..follow_args()
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::Follow(args),
                },
            )
            .await
            .unwrap(),
            json!({"success":true,"copied":true,"jobId":"job-2"})
        );
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/backtest/get-task-info"))
        );
        assert_eq!(
            body_of(&rx.recv().unwrap()),
            json!({
                "chainId":"56","type":"meme-rush",
                "config":{"filter":{"minHolders":10},"backtest":{"time_range":"30d","enabled":true}}
            })
        );
        rx.recv().unwrap();
        let created = rx.recv().unwrap();
        assert_eq!(
            request_line(&created),
            post(&format!("{BASE_PATH}/config/create"))
        );
        assert_eq!(
            body_of(&created),
            json!({
                "type":"meme-rush","chainId":"56","strategyName":"a-very-long-strategy",
                "config":{"filter":{"minHolders":10},"backtest":{"time_range":"30d","enabled":true}}
            })
        );
        let follow = rx.recv().unwrap();
        assert_eq!(
            body_of(&follow),
            json!({"chainId":"56","type":"meme-rush","jobId":"job-2"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_unfollow_looks_up_follows_then_posts_strategy_id() {
        let (origin, rx, server) = serve(vec![
            data(json!([{"strategy_id":"strategy-1","job_id":"job-1"}])),
            data(task_row("job-1")),
            data(json!({"unfollowed":true})),
        ]);
        let mut client = client(&origin);
        let args = StrategyUnfollowArgs {
            chain_id: "56".into(),
            strategy_type: "fomo".into(),
            strategy_id: "strategy-1".into(),
            yes: true,
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::Unfollow(args),
                },
            )
            .await
            .unwrap(),
            json!({"success": true})
        );
        let follows = rx.recv().unwrap();
        assert_eq!(
            request_line(&follows),
            post(&format!("{BASE_PATH}/strategy/list-my-follows"))
        );
        assert_eq!(body_of(&follows), json!({"chainId":"56"}));
        assert_eq!(
            request_line(&rx.recv().unwrap()),
            post(&format!("{BASE_PATH}/backtest/list-task-stats"))
        );
        let unfollow = rx.recv().unwrap();
        assert_eq!(
            request_line(&unfollow),
            post(&format!("{BASE_PATH}/strategy/unfollow"))
        );
        assert_eq!(
            body_of(&unfollow),
            json!({"chainId":"56","type":"fomo-call","strategyId":"strategy-1"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_list_walks_both_types_and_projects_owned_rows() {
        let (origin, rx, server) = serve(vec![
            data(json!({"taskList":[
                {"strategyId":"s1","taskName":"one","strategyType":"meme-rush","jobId":"j1","taskId":"1","followed":true,"isOwner":true},
                {"strategyId":"s2","taskName":"hall","type":"meme-rush","jobId":"j2","taskId":"2","followed":false,"isOwner":false}
            ],"total":2,"totalPages":1,"page":1,"size":100})),
            data(json!({"taskList":[
                {"strategyId":"s3","taskName":"two","strategyType":"fomo-call","jobId":"j3","taskId":"3","followed":true,"isOwner":true}
            ],"total":1,"totalPages":1,"page":1,"size":100})),
        ]);
        let mut client = client(&origin);
        let args = StrategyListArgs {
            chain_id: "56".into(),
            followed: false,
            strategy_type: None,
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::List(args),
                },
            )
            .await
            .unwrap(),
            json!([
                {"strategy_id":"s1","strategy_name":"one","strategy_type":"meme-rush","job_id":"j1","task_id":"1","followed":true},
                {"strategy_id":"s3","strategy_name":"two","strategy_type":"fomo-call","job_id":"j3","task_id":"3","followed":true}
            ])
        );
        let first = rx.recv().unwrap();
        assert_eq!(
            body_of(&first),
            json!({"chainId":"56","page":1,"size":100,"strategyType":"meme-rush"})
        );
        let second = rx.recv().unwrap();
        assert_eq!(
            body_of(&second),
            json!({"chainId":"56","page":1,"size":100,"strategyType":"fomo-call"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn strategy_list_followed_returns_backend_follow_rows() {
        let rows = json!([{"strategy_id":"s1","strategy_name":"one","strategy_type":"meme-rush","job_id":"j1","task_id":"1"}]);
        let (origin, rx, server) = serve(vec![data(rows.clone())]);
        let mut client = client(&origin);
        let args = StrategyListFollowedArgs {
            chain_id: "56".into(),
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Strategy {
                    command: StrategyCommand::ListFollowed(args),
                },
            )
            .await
            .unwrap(),
            rows
        );
        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            post(&format!("{BASE_PATH}/strategy/list-my-follows"))
        );
        assert_eq!(body_of(&request), json!({"chainId":"56"}));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backtest_list_uses_documented_page_and_size_defaults() {
        let page = json!({"taskList":[{"jobId":"j1"}],"page":1,"size":20,"total":1,"totalPages":1});
        let (origin, rx, server) = serve(vec![data(page.clone())]);
        let mut client = client(&origin);
        let args = BacktestListArgs {
            chain_id: "56".into(),
            page: "1".into(),
            size: "20".into(),
            backtest_days: Some("7".into()),
            all: false,
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::List(args),
                },
            )
            .await
            .unwrap(),
            page
        );
        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            post(&format!("{BASE_PATH}/backtest/list-task-stats"))
        );
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","page":1,"size":20,"backtestDays":7})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backtest_list_all_walks_every_page() {
        let (origin, rx, server) = serve(vec![
            data(json!({"taskList":[{"jobId":"j1"}],"page":1,"size":100,"total":2,"totalPages":2})),
            data(json!({"taskList":[{"jobId":"j2"}],"page":2,"size":100,"total":2,"totalPages":2})),
        ]);
        let mut client = client(&origin);
        let args = BacktestListArgs {
            chain_id: "56".into(),
            page: "5".into(),
            size: "1".into(),
            backtest_days: None,
            all: true,
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::List(args),
                },
            )
            .await
            .unwrap(),
            json!({"total":2,"taskList":[{"jobId":"j1"},{"jobId":"j2"}]})
        );
        for page in 1..=2 {
            assert_eq!(
                body_of(&rx.recv().unwrap()),
                json!({"chainId":"56","page":page,"size":100})
            );
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backtest_detail_returns_info_and_token_list() {
        let info = json!({
            "strategyId":"strategy-1","taskName":"synthetic","strategyType":"meme-rush",
            "status":"COMPLETED","tokenDetails":[{"symbol":"RWA","tokenName":"Real World"}]
        });
        let (origin, rx, server) = serve(vec![data(info.clone())]);
        let mut client = client(&origin);
        let args = BacktestDetailArgs {
            chain_id: "56".into(),
            strategy_id: "strategy-1".into(),
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::Detail(args),
                },
            )
            .await
            .unwrap(),
            json!({"info":info,"tokens":[{"symbol":"RWA","tokenName":"Real World"}]})
        );
        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            post(&format!("{BASE_PATH}/backtest/get-task-vo"))
        );
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","strategyId":"strategy-1"})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backtest_retry_sends_the_default_type_and_job_ids() {
        let (origin, rx, server) = serve(vec![data(json!({"successJobIds":["job-1"]}))]);
        let mut client = client(&origin);
        let args = BacktestRetryArgs {
            chain_id: "56".into(),
            strategy_type: "meme-rush".into(),
            job_id: "job-1".into(),
        };

        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::Retry(args),
                },
            )
            .await
            .unwrap(),
            json!({"success": true})
        );
        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            post(&format!("{BASE_PATH}/backtest/retry-job"))
        );
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","type":"meme-rush","jobIds":["job-1"]})
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backtest_schedule_configures_then_queries() {
        let (origin, rx, server) = serve(vec![
            data(json!({"next_run_time":1700000000})),
            data(json!({"schedule_interval":"4H","next_run_time":1700000000})),
        ]);
        let mut client = client(&origin);

        let configured = BacktestScheduleArgs {
            chain_id: "56".into(),
            job_id: "job-1".into(),
            interval: Some("4H".into()),
        };
        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::Schedule(configured),
                },
            )
            .await
            .unwrap(),
            json!({"next_run_time":1700000000})
        );
        let config = rx.recv().unwrap();
        assert_eq!(
            request_line(&config),
            post(&format!("{BASE_PATH}/schedule/config"))
        );
        assert_eq!(
            body_of(&config),
            json!({"chainId":"56","jobId":"job-1","scheduleInterval":"4H"})
        );

        let queried = BacktestScheduleArgs {
            chain_id: "56".into(),
            job_id: "job-1".into(),
            interval: None,
        };
        assert_eq!(
            run(
                &mut client,
                &SignalCommand::Backtest {
                    command: BacktestCommand::Schedule(queried),
                },
            )
            .await
            .unwrap(),
            json!({"schedule_interval":"4H","next_run_time":1700000000})
        );
        let query = rx.recv().unwrap();
        assert_eq!(
            request_line(&query),
            post(&format!("{BASE_PATH}/schedule/query"))
        );
        assert_eq!(body_of(&query), json!({"chainId":"56","jobId":"job-1"}));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn explore_returns_the_official_task_list() {
        let tasks = json!([{"strategyId":"official-1","taskName":"official","status":"RUNNING"}]);
        let (origin, rx, server) = serve(vec![data(json!({"taskList":tasks.clone()}))]);
        let mut client = client(&origin);
        let args = ExploreArgs {
            chain_id: "56".into(),
            backtest_days: Some("30".into()),
        };

        assert_eq!(
            run(&mut client, &SignalCommand::Explore(args))
                .await
                .unwrap(),
            tasks
        );
        let request = rx.recv().unwrap();
        assert_eq!(request_line(&request), post(EXPLORE_PATH));
        assert_eq!(body_of(&request), json!({"chainId":"56","backtestDays":30}));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn credits_posts_an_empty_body() {
        let credits = json!({"balance":3,"dailyLimit":5,"totalVolume":"12","isWhiteList":true});
        let (origin, rx, server) = serve(vec![data(credits.clone())]);
        let mut client = client(&origin);

        assert_eq!(
            run(&mut client, &SignalCommand::Credits).await.unwrap(),
            credits
        );
        let request = rx.recv().unwrap();
        assert_eq!(
            request_line(&request),
            post(&format!("{BASE_PATH}/credits/info"))
        );
        assert_eq!(body_of(&request), json!({}));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn wallet_group_requests_every_group() {
        let groups = json!([{"groupId":"1","groupName":"main","addressCount":2}]);
        let (origin, rx, server) = serve(vec![data(groups.clone())]);
        let mut client = client(&origin);
        let args = WalletGroupArgs {
            chain_id: "56".into(),
        };

        assert_eq!(
            run(&mut client, &SignalCommand::WalletGroup(args))
                .await
                .unwrap(),
            groups
        );
        let request = rx.recv().unwrap();
        assert_eq!(request_line(&request), post(WALLET_GROUP_PATH));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","includeAllGroup":true})
        );
        server.join().unwrap();
    }

    #[test]
    fn mutation_classification_covers_every_leaf_both_ways() {
        let reads = vec![
            SignalCommand::List(list_args()),
            SignalCommand::Strategy {
                command: StrategyCommand::List(StrategyListArgs {
                    chain_id: "56".into(),
                    followed: false,
                    strategy_type: None,
                }),
            },
            SignalCommand::Strategy {
                command: StrategyCommand::ListFollowed(StrategyListFollowedArgs {
                    chain_id: "56".into(),
                }),
            },
            SignalCommand::Backtest {
                command: BacktestCommand::List(BacktestListArgs {
                    chain_id: "56".into(),
                    page: "1".into(),
                    size: "20".into(),
                    backtest_days: None,
                    all: false,
                }),
            },
            SignalCommand::Backtest {
                command: BacktestCommand::Detail(BacktestDetailArgs {
                    chain_id: "56".into(),
                    strategy_id: "strategy-1".into(),
                }),
            },
            SignalCommand::Explore(ExploreArgs {
                chain_id: "56".into(),
                backtest_days: None,
            }),
            SignalCommand::Credits,
            SignalCommand::WalletGroup(WalletGroupArgs {
                chain_id: "56".into(),
            }),
        ];
        let writes = vec![
            SignalCommand::Strategy {
                command: StrategyCommand::Create(create_args()),
            },
            SignalCommand::Strategy {
                command: StrategyCommand::Update(StrategyUpdateArgs {
                    name: Some("renamed".into()),
                    ..update_args()
                }),
            },
            SignalCommand::Strategy {
                command: StrategyCommand::Delete(StrategyDeleteArgs {
                    chain_id: "56".into(),
                    strategy_type: "meme-rush".into(),
                    job_id: "job-1".into(),
                    yes: true,
                }),
            },
            SignalCommand::Strategy {
                command: StrategyCommand::Follow(follow_args()),
            },
            SignalCommand::Strategy {
                command: StrategyCommand::Unfollow(StrategyUnfollowArgs {
                    chain_id: "56".into(),
                    strategy_type: "meme-rush".into(),
                    strategy_id: "strategy-1".into(),
                    yes: true,
                }),
            },
            SignalCommand::Backtest {
                command: BacktestCommand::Retry(BacktestRetryArgs {
                    chain_id: "56".into(),
                    strategy_type: "meme-rush".into(),
                    job_id: "job-1".into(),
                }),
            },
            SignalCommand::Backtest {
                command: BacktestCommand::Schedule(BacktestScheduleArgs {
                    chain_id: "56".into(),
                    job_id: "job-1".into(),
                    interval: Some("4H".into()),
                }),
            },
        ];
        assert_eq!(reads.len() + writes.len(), 15);
        for command in &reads {
            assert!(!is_mutation(command), "{command:?} must be read-only");
            assert!(mutation_payload(command).is_err());
        }
        for command in &writes {
            assert!(is_mutation(command), "{command:?} must be a mutation");
            let payload = mutation_payload(command).unwrap();
            assert_eq!(payload["chainId"], "56");
            let text = payload.to_string();
            assert!(!text.contains("synthetic-session") && !text.contains("synthetic-client"));
        }
        assert_eq!(
            mutation_payload(&SignalCommand::Strategy {
                command: StrategyCommand::Unfollow(StrategyUnfollowArgs {
                    chain_id: "56".into(),
                    strategy_type: "fomo".into(),
                    strategy_id: "strategy-1".into(),
                    yes: true,
                }),
            })
            .unwrap(),
            json!({"action":"strategy.unfollow","chainId":"56","type":"fomo-call","strategyId":"strategy-1"})
        );
        assert_eq!(
            mutation_payload(&SignalCommand::Backtest {
                command: BacktestCommand::Schedule(BacktestScheduleArgs {
                    chain_id: "56".into(),
                    job_id: "job-1".into(),
                    interval: Some("OFF".into()),
                }),
            })
            .unwrap(),
            json!({"action":"backtest.schedule","chainId":"56","jobId":"job-1","interval":"OFF"})
        );
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let (origin, rx, server) = serve(vec![json!({
            "code":"61001001","message":"do-not-echo-backend-text","data":null
        })]);
        let mut client = client(&origin);

        let error = run(
            &mut client,
            &SignalCommand::WalletGroup(WalletGroupArgs {
                chain_id: "56".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Binance API code 61001001");
        assert!(!error.contains("do-not-echo-backend-text"));
        rx.recv().unwrap();
        server.join().unwrap();
    }

    #[tokio::test]
    async fn missing_or_invalid_options_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut client = client(&origin);

        let empty_chain = SignalCommand::WalletGroup(WalletGroupArgs {
            chain_id: String::new(),
        });
        assert_eq!(
            run(&mut client, &empty_chain).await.unwrap_err(),
            "invalid --chain-id"
        );

        let missing_strategy = SignalCommand::Backtest {
            command: BacktestCommand::Detail(BacktestDetailArgs {
                chain_id: "56".into(),
                strategy_id: String::new(),
            }),
        };
        assert_eq!(
            run(&mut client, &missing_strategy).await.unwrap_err(),
            "invalid --strategy-id"
        );

        let empty_config = SignalCommand::Strategy {
            command: StrategyCommand::Create(StrategyCreateArgs {
                config: "{}".into(),
                ..create_args()
            }),
        };
        assert!(run(&mut client, &empty_config)
            .await
            .unwrap_err()
            .contains("empty config"));

        let bad_interval = SignalCommand::Backtest {
            command: BacktestCommand::Schedule(BacktestScheduleArgs {
                chain_id: "56".into(),
                job_id: "job-1".into(),
                interval: Some("2H".into()),
            }),
        };
        assert!(run(&mut client, &bad_interval)
            .await
            .unwrap_err()
            .contains("--interval"));

        let long_name = SignalCommand::Strategy {
            command: StrategyCommand::Create(StrategyCreateArgs {
                name: "x".repeat(21),
                ..create_args()
            }),
        };
        assert!(run(&mut client, &long_name)
            .await
            .unwrap_err()
            .contains("20 characters"));

        assert!(
            listener.accept().is_err(),
            "invalid options must not reach the network"
        );
    }
}
