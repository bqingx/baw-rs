//! Wallet tracker: groups, addresses, monitors. Mirrors `baw tracker`.
//!
//! The tracker endpoints sit on two prefixes that are *not* `client::API`, so they
//! are passed as full paths. From the installed `baw` 1.12.0 bundle
//! (`@binance/agentic-wallet/dist/index.js`):
//!
//! ```text
//! var At="/bapi/defi/v1/public/wallet-direct",
//!     ne="/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet",
//!     ds="/bapi/defi/v1/public/wallet-direct/tracker/wallet";
//! ```
//!
//! `ne` is the authenticated agent tracker base ([`TRACKER`]), `ds` the public
//! SMY/KOL base ([`TRACKER_PUBLIC`]). Every JS handler opens with `await I()`
//! (session check) even for the public-mode queries, so every leaf here requires a
//! session before it reaches the network.
//!
//! Output shapes follow the JS `--json` path (`g(...)`) for each leaf; the JS
//! deliberately reshapes the token/tx monitors and the write results, and those
//! projections are reproduced (each one carries the quoted source line).
//!
//! `tracker ws` is intentionally not implemented: it needs a WebSocket client and
//! this crate has no ws dependency. It fails with a static, explicit error.
use crate::client::Client;
use clap::Args;
use reqwest::Method;
use serde_json::{json, Number, Value};

/// `ne="/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet"` — agent tracker base.
const TRACKER: &str = "/bapi/defi/v1/public/wallet-direct/agent/tracker/wallet";
/// `ds="/bapi/defi/v1/public/wallet-direct/tracker/wallet"` — public SMY/KOL base.
const TRACKER_PUBLIC: &str = "/bapi/defi/v1/public/wallet-direct/tracker/wallet";

const WS_UNIMPLEMENTED: &str = "tracker ws requires websocket support, not implemented";

/// `var Wa=new Set(["1","56","8453","CT_501","4663"])`, enforced by `function $(t)`.
/// The bundle's own error text and the ws chain map
/// (`Cr={56:"BSC",1:"ETH",8453:"BASE",CT_501:"SOL",4663:"ROBINHOOD",5042:"ARC"}`)
/// both advertise 5042 (ARC) although the set omits it; 5042 is accepted here
/// rather than reproducing that omission. Everything else is refused before the
/// network.
const CHAINS: [&str; 6] = ["1", "56", "8453", "CT_501", "4663", "5042"];

/// `var kt=1e4` — page ceiling inside `function Ge(t)`.
const MAX_PAGE: u32 = 10_000;
/// `var Ba=20,nt=100,kt=1e4` — page-size ceiling inside `function re(t)`.
const MAX_PAGE_SIZE: u32 = 100;
/// `var Vl=["1m","5m","1h","4h","24h"]` — the accepted `--period` values.
const PERIODS: [&str; 5] = ["1m", "5m", "1h", "4h", "24h"];

const INVALID_CHAIN: &str = "invalid chain ID; supported: 1 (ETH), 56 (BSC), 8453 (Base), \
                             CT_501 (Solana), 4663 (Robinhood), 5042 (ARC)";
const INVALID_GROUP: &str = "invalid group-id; expected a positive integer";
const INVALID_PERIOD: &str = "invalid period; valid: 1m, 5m, 1h, 4h, 24h";
const INVALID_TAG: &str = "invalid tag-type; valid: kol, smy";
const INVALID_LABEL: &str = "invalid label";
const INVALID_ADDRESS: &str = "invalid address";
const INVALID_JSON_ADDRESSES: &str =
    "invalid JSON in --addresses; expected an array of {address,label} objects";
const NO_ADDRESSES: &str = "no addresses provided";

// ---------------------------------------------------------------------------
// clap surface — option names, value names, defaults and required-ness are the
// installed CLI's own `--help` text (docs/JS-CLI-PARITY.md) verbatim.
// ---------------------------------------------------------------------------

#[derive(clap::Subcommand, Debug)]
pub enum TrackerCommand {
    /// Token-dimension monitor (agent own group / public SMY-KOL)
    Token(TokenArgs),
    /// Transaction-dimension monitor (agent own group / public SMY-KOL)
    Tx(TxArgs),
    /// List addresses the current user follows (read-only)
    Follow(FollowArgs),
    /// Manage tracker address groups
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Manage tracker addresses
    Address {
        #[command(subcommand)]
        command: AddressCommand,
    },
    /// Subscribe to WSP push events via WebSocket
    Ws,
}

#[derive(clap::Subcommand, Debug)]
pub enum GroupCommand {
    /// Create a new group
    Create(GroupCreateArgs),
    /// List address groups
    List(GroupListArgs),
    /// Rename a group
    Update(GroupUpdateArgs),
}

#[derive(clap::Subcommand, Debug)]
pub enum AddressCommand {
    /// Add a single address (wraps import)
    Add(AddressAddArgs),
    /// Batch import addresses into a group
    Batch(AddressBatchArgs),
    /// Delete address(es) from a group (supports batch)
    Delete(AddressDeleteArgs),
    /// Follow an address (G9-A)
    Follow(AddressFollowArgs),
    /// Link an address to a group (address/group/link)
    Link(AddressLinkArgs),
    /// Paginated address list (address/page)
    List(AddressListArgs),
    /// Fuzzy search addresses within a group
    Search(AddressSearchArgs),
    /// Unfollow an address (G9-A)
    Unfollow(AddressUnfollowArgs),
    /// Update address label/note (merges address/update + address/label/update)
    Update(AddressUpdateArgs),
}

#[derive(Args, Debug)]
pub struct TokenArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID (agent own group)
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Public mode tag: kol / smy (omit for agent own-group mode)
    #[arg(long = "tag-type", value_name = "type")]
    pub tag_type: Option<String>,
    /// Number of recent tokens (default 70)
    #[arg(long = "token-size", value_name = "n")]
    pub token_size: Option<String>,
    /// Period: 1m/5m/1h/4h/24h (default 24h)
    #[arg(long = "period", value_name = "period", default_value = "24h")]
    pub period: String,
    /// Filter risk tokens
    #[arg(long = "filter-risk")]
    pub filter_risk: bool,
}

#[derive(Args, Debug)]
pub struct TxArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID (agent own group)
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Public mode tag: kol / smy (omit for agent own-group mode)
    #[arg(long = "tag-type", value_name = "type")]
    pub tag_type: Option<String>,
    /// Comma-separated: 19=buy 11=first-buy 29=sell 21=clear
    #[arg(long = "trade-side", value_name = "sides")]
    pub trade_side: Option<String>,
    /// Min tx USD value
    #[arg(long = "min-value", value_name = "n")]
    pub min_value: Option<String>,
    /// Max tx USD value
    #[arg(long = "max-value", value_name = "n")]
    pub max_value: Option<String>,
    /// Filter risk tokens
    #[arg(long = "filter-risk")]
    pub filter_risk: bool,
}

#[derive(Args, Debug)]
pub struct FollowArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
}

#[derive(Args, Debug)]
pub struct GroupCreateArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group name
    #[arg(short = 'n', long = "name", value_name = "name")]
    pub name: String,
}

#[derive(Args, Debug)]
pub struct GroupListArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Exclude the "All" virtual group
    #[arg(long = "no-all-group")]
    pub no_all_group: bool,
}

#[derive(Args, Debug)]
pub struct GroupUpdateArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: String,
    /// New group name
    #[arg(short = 'n', long = "name", value_name = "name")]
    pub name: String,
}

#[derive(Args, Debug)]
pub struct AddressAddArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Address
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: String,
    /// Label
    #[arg(short = 'l', long = "label", value_name = "label")]
    pub label: Option<String>,
    /// Group ID (default group if omitted)
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Do not overwrite existing label/emoji
    #[arg(long = "no-overwrite")]
    pub no_overwrite: bool,
}

#[derive(Args, Debug)]
pub struct AddressBatchArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Comma-separated addresses, or JSON array of {address,label}
    #[arg(short = 'a', long = "addresses", value_name = "list")]
    pub addresses: String,
    /// Group ID (default group if omitted)
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Do not overwrite existing label/emoji
    #[arg(long = "no-overwrite")]
    pub no_overwrite: bool,
}

#[derive(Args, Debug)]
pub struct AddressDeleteArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: String,
    /// Comma-separated addresses to delete
    #[arg(short = 'a', long = "addresses", value_name = "list")]
    pub addresses: String,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct AddressFollowArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Address
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: String,
    /// Group ID
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Label
    #[arg(short = 'l', long = "label", value_name = "label")]
    pub label: Option<String>,
}

#[derive(Args, Debug)]
pub struct AddressLinkArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Address
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: String,
    /// Target group ID (default group if omitted)
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: Option<String>,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct AddressListArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: String,
    /// Page number (from 1)
    #[arg(long = "page", value_name = "page", default_value = "1")]
    pub page: String,
    /// Page size (max 100)
    #[arg(long = "size", value_name = "size", default_value = "20")]
    pub size: String,
}

#[derive(Args, Debug)]
pub struct AddressSearchArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Group ID
    #[arg(short = 'g', long = "group-id", value_name = "groupId")]
    pub group_id: String,
    /// Address filter (fuzzy)
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: Option<String>,
    /// Label filter (fuzzy)
    #[arg(short = 'l', long = "label", value_name = "label")]
    pub label: Option<String>,
}

#[derive(Args, Debug)]
pub struct AddressUnfollowArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Address
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: String,
}

#[derive(Args, Debug)]
pub struct AddressUpdateArgs {
    /// Chain ID (56=BSC, 1=ETH, 8453=BASE, CT_501=SOL)
    #[arg(short = 'c', long = "chain-id", value_name = "chainId")]
    pub chain_id: String,
    /// Address
    #[arg(short = 'a', long = "address", value_name = "address")]
    pub address: String,
    /// New label/note
    #[arg(short = 'l', long = "label", value_name = "label")]
    pub label: String,
}

// ---------------------------------------------------------------------------
// classification / journal payload
// ---------------------------------------------------------------------------

/// Reads: `token`, `tx`, `follow`, `group list`, `address list`, `address search`
/// (and the unimplemented `ws`). Writes: `group create|update` and every address
/// leaf except list/search. `dispatch.rs` keys `--confirm`/`--operation-id` and the
/// durable journal off this split, so it must stay exact.
pub fn is_mutation(command: &TrackerCommand) -> bool {
    match command {
        TrackerCommand::Group { command } => {
            matches!(command, GroupCommand::Create(_) | GroupCommand::Update(_))
        }
        TrackerCommand::Address { command } => matches!(
            command,
            AddressCommand::Add(_)
                | AddressCommand::Batch(_)
                | AddressCommand::Delete(_)
                | AddressCommand::Follow(_)
                | AddressCommand::Link(_)
                | AddressCommand::Unfollow(_)
                | AddressCommand::Update(_)
        ),
        TrackerCommand::Token(_)
        | TrackerCommand::Tx(_)
        | TrackerCommand::Follow(_)
        | TrackerCommand::Ws => false,
    }
}

/// Deterministic intent digest input for every writing leaf: identifiers, addresses
/// and group ids only, taken from argv — never credentials and never anything read
/// back from a response. The `args` object is the exact wire body the leaf sends.
pub fn mutation_payload(command: &TrackerCommand) -> Result<Value, String> {
    match command {
        TrackerCommand::Group {
            command: GroupCommand::Create(args),
        } => Ok(json!({
            "operation": "tracker.group.create",
            "args": group_create_body(args)?,
        })),
        TrackerCommand::Group {
            command: GroupCommand::Update(args),
        } => Ok(json!({
            "operation": "tracker.group.update",
            "args": group_update_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Add(args),
        } => Ok(json!({
            "operation": "tracker.address.add",
            "args": address_add_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Batch(args),
        } => Ok(json!({
            "operation": "tracker.address.batch",
            "args": address_batch_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Delete(args),
        } => Ok(json!({
            "operation": "tracker.address.delete",
            "args": address_delete_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Follow(args),
        } => Ok(json!({
            "operation": "tracker.address.follow",
            "args": address_follow_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Link(args),
        } => Ok(json!({
            "operation": "tracker.address.link",
            "args": address_link_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Unfollow(args),
        } => Ok(json!({
            "operation": "tracker.address.unfollow",
            "args": address_unfollow_body(args)?,
        })),
        TrackerCommand::Address {
            command: AddressCommand::Update(args),
        } => Ok(json!({
            "operation": "tracker.address.update",
            "args": address_update_body(args)?,
        })),
        _ => Err("not a mutation".into()),
    }
}

pub async fn run(client: &mut Client, command: &TrackerCommand) -> Result<Value, String> {
    if matches!(command, TrackerCommand::Ws) {
        return Err(WS_UNIMPLEMENTED.into());
    }
    session(client)?;
    match command {
        TrackerCommand::Token(args) => token(client, args).await,
        TrackerCommand::Tx(args) => tx(client, args).await,
        TrackerCommand::Follow(args) => followings(client, args).await,
        TrackerCommand::Group { command } => match command {
            GroupCommand::Create(args) => group_create(client, args).await,
            GroupCommand::List(args) => group_list(client, args).await,
            GroupCommand::Update(args) => group_update(client, args).await,
        },
        TrackerCommand::Address { command } => match command {
            AddressCommand::Add(args) => address_add(client, args).await,
            AddressCommand::Batch(args) => address_batch(client, args).await,
            AddressCommand::Delete(args) => address_delete(client, args).await,
            AddressCommand::Follow(args) => address_follow(client, args).await,
            AddressCommand::Link(args) => address_link(client, args).await,
            AddressCommand::List(args) => address_list(client, args).await,
            AddressCommand::Search(args) => address_search(client, args).await,
            AddressCommand::Unfollow(args) => address_unfollow(client, args).await,
            AddressCommand::Update(args) => address_update(client, args).await,
        },
        // Returned above; kept so the match stays exhaustive without `unreachable!`.
        TrackerCommand::Ws => Err(WS_UNIMPLEMENTED.into()),
    }
}

// ---------------------------------------------------------------------------
// reads
// ---------------------------------------------------------------------------

async fn token(client: &mut Client, args: &TokenArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let period = period(&args.period)?;
    let mut body = json!({
        "chainId": chain,
        "period": period,
        "enableFilterRiskTokens": args.filter_risk,
    });
    if let Some(size) = &args.token_size {
        body["tokenSize"] = json!(token_size(size)?);
    }
    match tag_type_option(&args.tag_type)? {
        Some(tag) => {
            // `let{groupId:o,...s}=e,i=await this.api.queryTokenPublic({...s,tagType:n})`:
            // public mode drops `--group-id` and sends the numeric tag instead.
            body["tagType"] = json!(tag);
            // `El=ds+"/token/query"`, `queryTokenPublic(e){return this.request(El,{method:"POST",body:e})}`
            let data = client
                .request(
                    Method::POST,
                    &format!("{TRACKER_PUBLIC}/token/query"),
                    Some(body),
                )
                .await?;
            token_rows(&data)
        }
        None => {
            let group = args.group_id.as_deref().ok_or(
                "either --group-id (agent own group) or --tag-type (public SMY/KOL) is required",
            )?;
            body["groupId"] = json!(group_id(group)?);
            // `Rl=ne+"/token/query"`, `queryTokenAgent(e){return this.request(Rl,{method:"POST",body:e})}`
            let data = client
                .request(Method::POST, &format!("{TRACKER}/token/query"), Some(body))
                .await?;
            token_rows(&data)
        }
    }
}

async fn tx(client: &mut Client, args: &TxArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    let mut body = json!({
        "chainId": chain,
        "enableFilterRiskTokens": args.filter_risk,
    });
    if let Some(group) = &args.group_id {
        body["groupId"] = json!(group_id(group)?);
    }
    if let Some(sides) = &args.trade_side {
        body["tradeSideCategory"] = json!(trade_sides(sides)?);
    }
    if let Some(value) = &args.min_value {
        body["minTxValue"] = json!(usd(
            value,
            "invalid --min-value; expected a non-negative number"
        )?);
    }
    if let Some(value) = &args.max_value {
        body["maxTxValue"] = json!(usd(
            value,
            "invalid --max-value; expected a non-negative number"
        )?);
    }
    match tag_type_option(&args.tag_type)? {
        Some(tag) => {
            // `return s=await this.api.queryAddressPublic({...e,tagType:n})` — unlike the
            // token leaf, public tx mode keeps `groupId` when it was supplied.
            body["tagType"] = json!(tag);
            // `Pl=ds+"/address/query"`, `queryAddressPublic(e){return this.request(Pl,{method:"POST",body:e})}`
            let data = client
                .request(
                    Method::POST,
                    &format!("{TRACKER_PUBLIC}/address/query"),
                    Some(body),
                )
                .await?;
            tx_rows(&data, true)
        }
        None => {
            if args.group_id.is_none() {
                return Err(
                    "either --group-id (agent own group) or --tag-type (public SMY/KOL) is required"
                        .into(),
                );
            }
            // `kl=ne+"/address/query"`, `queryAddressAgent(e){return this.request(kl,{method:"POST",body:e})}`
            let data = client
                .request(
                    Method::POST,
                    &format!("{TRACKER}/address/query"),
                    Some(body),
                )
                .await?;
            tx_rows(&data, false)
        }
    }
}

async fn followings(client: &mut Client, args: &FollowArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    // `vl=ne+"/address/followings"`, `getFollowings(e){return this.request(vl,{method:"POST",body:e})}`
    // called as `getFollowings({chainId:e})` with `?? {}` on a null body.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/followings"),
            Some(json!({ "chainId": chain })),
        )
        .await?;
    Ok(if data.is_null() { json!({}) } else { data })
}

async fn group_list(client: &mut Client, args: &GroupListArgs) -> Result<Value, String> {
    let chain = chain_id(&args.chain_id)?;
    // `Ol=ne+"/group/list"`, `listGroup(e){return this.request(Ol,{method:"POST",body:e})}` and
    // `listGroups(e,n=!0){...listGroup({chainId:e,includeAllGroup:n})}` driven by `n.allGroup!==!1`.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/group/list"),
            Some(json!({ "chainId": chain, "includeAllGroup": !args.no_all_group })),
        )
        .await?;
    Ok(if data.is_null() { json!([]) } else { data })
}

async fn address_list(client: &mut Client, args: &AddressListArgs) -> Result<Value, String> {
    let body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "groupId": group_id(&args.group_id)?,
        "page": page(&args.page)?,
        "pageSize": page_size(&args.size)?,
    });
    // `_l=ne+"/address/page"`, `pageAddress(e){return this.request(_l,{method:"POST",body:e})}`
    // with `?? {total:0,rows:[]}`.
    let data = client
        .request(Method::POST, &format!("{TRACKER}/address/page"), Some(body))
        .await?;
    Ok(paged(&data))
}

async fn address_search(client: &mut Client, args: &AddressSearchArgs) -> Result<Value, String> {
    let mut body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "groupId": group_id(&args.group_id)?,
    });
    if let Some(address) = &args.address {
        body["address"] = json!(address_value(address)?);
    }
    if let Some(label) = &args.label {
        body["label"] = json!(label_value(label)?);
    }
    // `xl=ne+"/address/search"`, `searchAddress(e){return this.request(xl,{method:"POST",body:e})}`
    // with `?? {total:0,rows:[]}`.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/search"),
            Some(body),
        )
        .await?;
    Ok(paged(&data))
}

// ---------------------------------------------------------------------------
// writes
// ---------------------------------------------------------------------------

async fn group_create(client: &mut Client, args: &GroupCreateArgs) -> Result<Value, String> {
    let body = group_create_body(args)?;
    let name = body["groupName"].clone();
    // `Ll=ne+"/group/create"`, `createGroup(e){return this.request(Ll,{method:"POST",body:e})}`
    let created = client
        .request(Method::POST, &format!("{TRACKER}/group/create"), Some(body))
        .await?;
    // `let o=this.unwrap(r,"Group create failed");if(!(await this.listGroups(e)).some(
    //  i=>i.groupId===o||i.groupName===n))throw ... "Group create not confirmed on readback"`.
    // A silent no-op create must not be reported as success.
    let groups = group_rows(client, &args.chain_id).await?;
    let confirmed = groups
        .iter()
        .any(|group| id_matches(group, &created) || group["groupName"] == name);
    if !confirmed {
        return Err("group create not confirmed on readback".into());
    }
    // `g({groupId:a,groupName:n.name})` — the JS prints the untrimmed argv name.
    Ok(json!({ "groupId": created, "groupName": args.name }))
}

async fn group_update(client: &mut Client, args: &GroupUpdateArgs) -> Result<Value, String> {
    let body = group_update_body(args)?;
    let id = group_id(&args.group_id)?;
    let name = body["groupName"].clone();
    // `Nl=ne+"/group/update"`, `updateGroup(e){return this.request(Nl,{method:"POST",body:e})}`
    // The update response itself is discarded: the JS returns the listed id/name.
    client
        .request(Method::POST, &format!("{TRACKER}/group/update"), Some(body))
        .await?;
    // `if(!(await this.listGroups(e)).some(a=>a.groupId===n&&a.groupName===r))
    //  throw ... "Group update not confirmed on readback"`.
    let groups = group_rows(client, &args.chain_id).await?;
    let confirmed = groups
        .iter()
        .any(|group| id_matches(group, &json!(id)) && group["groupName"] == name);
    if !confirmed {
        return Err("group update not confirmed on readback".into());
    }
    // `g({groupId:s,groupName:n.name})`
    Ok(json!({ "groupId": id, "groupName": args.name }))
}

async fn address_add(client: &mut Client, args: &AddressAddArgs) -> Result<Value, String> {
    let body = address_add_body(args)?;
    // `$l=ne+"/address/import"`, `importAddress(e){return this.request($l,{method:"POST",body:e})}`
    // via `addAddress(...)` → `{chainId,groupId,allowOverwrite,addressList:[{address,label?}]}`.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/import"),
            Some(body),
        )
        .await?;
    // `g({imported:s})`
    Ok(json!({ "imported": data }))
}

async fn address_batch(client: &mut Client, args: &AddressBatchArgs) -> Result<Value, String> {
    let body = address_batch_body(args)?;
    let count = body["addressList"].as_array().map_or(0, Vec::len);
    // `$l=ne+"/address/import"`, `importAddress(e){...}` via `batchImportAddresses` which
    // returns `{imported:unwrap(...),count:n.length,invalidAddresses:[]}`.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/import"),
            Some(body),
        )
        .await?;
    // The JS never populates `invalidAddresses` client-side; keep its empty array.
    Ok(json!({ "imported": data, "count": count, "invalidAddresses": [] }))
}

async fn address_delete(client: &mut Client, args: &AddressDeleteArgs) -> Result<Value, String> {
    let body = address_delete_body(args)?;
    let id = group_id(&args.group_id)?;
    let deleted = body["addressList"].clone();
    // `Fl=ne+"/address/delete"`, `deleteAddress(e){return this.request(Fl,{method:"POST",body:e})}`
    client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/delete"),
            Some(body),
        )
        .await?;
    // `g({groupId:o,deleted:s})`
    Ok(json!({ "groupId": id, "deleted": deleted }))
}

async fn address_follow(client: &mut Client, args: &AddressFollowArgs) -> Result<Value, String> {
    let body = address_follow_body(args)?;
    // `Gl=ne+"/address/follow"`, `followAddress(e){return this.request(Gl,{method:"POST",body:e})}`
    client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/follow"),
            Some(body),
        )
        .await?;
    // `g({address:e.address,followed:!0})`
    Ok(json!({ "address": args.address, "followed": true }))
}

async fn address_link(client: &mut Client, args: &AddressLinkArgs) -> Result<Value, String> {
    let body = address_link_body(args)?;
    // `Ul=ne+"/address/group/link"`, `linkAddressGroup(e){return this.request(Ul,{method:"POST",body:e})}`
    client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/group/link"),
            Some(body),
        )
        .await?;
    // `g({address:e.address,groupId:o})` — `undefined` groupId is dropped by JSON.stringify.
    let mut out = json!({ "address": args.address });
    if let Some(group) = &args.group_id {
        out["groupId"] = json!(group_id(group)?);
    }
    Ok(out)
}

async fn address_unfollow(
    client: &mut Client,
    args: &AddressUnfollowArgs,
) -> Result<Value, String> {
    let body = address_unfollow_body(args)?;
    // `Hl=ne+"/address/unfollow"`, `unfollowAddress(e){return this.request(Hl,{method:"POST",body:e})}`
    client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/unfollow"),
            Some(body),
        )
        .await?;
    // `g({address:e.address,followed:!1})`
    Ok(json!({ "address": args.address, "followed": false }))
}

async fn address_update(client: &mut Client, args: &AddressUpdateArgs) -> Result<Value, String> {
    let body = address_update_body(args)?;
    // `Ml=ne+"/address/update"`, `updateAddress(e){return this.request(Ml,{method:"POST",body:e})}`
    let primary = client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/update"),
            Some(body.clone()),
        )
        .await;
    if primary.is_ok() {
        return Ok(json!({ "address": args.address, "label": args.label }));
    }
    // A 401/403 zeroizes the session; retrying would only mask the invalidation.
    if client.credentials().session_id.is_empty() {
        return primary;
    }
    // `ql=ne+"/address/label/update"`, `updateAddressLabel(e){return this.request(ql,{method:"POST",body:e})}`
    // The JS races both with `Promise.allSettled` and accepts either success: `i=o.status==="fulfilled"
    // &&o.value.code==="000000"` / `a=s.status==="fulfilled"&&s.value.code==="000000"`.
    match client
        .request(
            Method::POST,
            &format!("{TRACKER}/address/label/update"),
            Some(body),
        )
        .await
    {
        Ok(_) => Ok(json!({ "address": args.address, "label": args.label })),
        Err(second) => {
            if client.credentials().session_id.is_empty() {
                Err(second)
            } else {
                // `throw new m(d.TRACKER_API_ERROR,\`Address update failed: ${l}\`)` — the JS
                // interpolates the upstream reason; we keep the message static.
                Err("address update failed on both address/update and address/label/update".into())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// wire bodies (shared by `run` and `mutation_payload`)
// ---------------------------------------------------------------------------

fn group_create_body(args: &GroupCreateArgs) -> Result<Value, String> {
    Ok(json!({
        "chainId": chain_id(&args.chain_id)?,
        "groupName": group_name(&args.name)?,
    }))
}

fn group_update_body(args: &GroupUpdateArgs) -> Result<Value, String> {
    Ok(json!({
        "chainId": chain_id(&args.chain_id)?,
        "groupId": group_id(&args.group_id)?,
        "groupName": group_name(&args.name)?,
    }))
}

fn address_add_body(args: &AddressAddArgs) -> Result<Value, String> {
    let mut entry = json!({ "address": address_value(&args.address)? });
    if let Some(label) = &args.label {
        entry["label"] = json!(label_value(label)?);
    }
    let mut body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "allowOverwrite": !args.no_overwrite,
        "addressList": [entry],
    });
    if let Some(group) = &args.group_id {
        body["groupId"] = json!(group_id(group)?);
    }
    Ok(body)
}

fn address_batch_body(args: &AddressBatchArgs) -> Result<Value, String> {
    let mut body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "allowOverwrite": !args.no_overwrite,
        "addressList": batch_addresses(&args.addresses)?,
    });
    if let Some(group) = &args.group_id {
        body["groupId"] = json!(group_id(group)?);
    }
    Ok(body)
}

fn address_delete_body(args: &AddressDeleteArgs) -> Result<Value, String> {
    Ok(json!({
        "chainId": chain_id(&args.chain_id)?,
        "groupId": group_id(&args.group_id)?,
        "addressList": csv_addresses(&args.addresses)?,
    }))
}

fn address_follow_body(args: &AddressFollowArgs) -> Result<Value, String> {
    let mut body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "address": address_value(&args.address)?,
    });
    if let Some(group) = &args.group_id {
        body["groupId"] = json!(group_id(group)?);
    }
    if let Some(label) = &args.label {
        body["label"] = json!(label_value(label)?);
    }
    Ok(body)
}

fn address_link_body(args: &AddressLinkArgs) -> Result<Value, String> {
    let mut body = json!({
        "chainId": chain_id(&args.chain_id)?,
        "address": address_value(&args.address)?,
    });
    if let Some(group) = &args.group_id {
        body["groupId"] = json!(group_id(group)?);
    }
    Ok(body)
}

fn address_unfollow_body(args: &AddressUnfollowArgs) -> Result<Value, String> {
    Ok(json!({
        "chainId": chain_id(&args.chain_id)?,
        "address": address_value(&args.address)?,
    }))
}

fn address_update_body(args: &AddressUpdateArgs) -> Result<Value, String> {
    Ok(json!({
        "chainId": chain_id(&args.chain_id)?,
        "address": address_value(&args.address)?,
        "label": label_value(&args.label)?,
    }))
}

async fn group_rows(client: &mut Client, chain: &str) -> Result<Vec<Value>, String> {
    // `listGroups(e,n=!0)` posts `{chainId:e,includeAllGroup:n}` to `Ol=ne+"/group/list"`.
    let data = client
        .request(
            Method::POST,
            &format!("{TRACKER}/group/list"),
            Some(json!({ "chainId": chain_id(chain)?, "includeAllGroup": true })),
        )
        .await?;
    Ok(data.as_array().cloned().unwrap_or_default())
}

// ---------------------------------------------------------------------------
// response projections
// ---------------------------------------------------------------------------

/// `return r.map(o=>{let s=o.addressList??[];return{...o,consensusCount:s.length,
///  netInflow:s.reduce((i,a)=>i+(a.inflow??0),0),countBuy:s.reduce((i,a)=>i+(a.buyCount??0),0),
///  countSell:s.reduce((i,a)=>i+(a.sellCount??0),0)}})`
///
/// The JS sums with `+`, which concatenates when a field arrives as a string; the
/// numeric value is used here for string inputs so the totals stay numbers.
fn token_rows(data: &Value) -> Result<Value, String> {
    let rows = data
        .as_array()
        .ok_or("invalid token monitor response; expected an array")?;
    Ok(Value::Array(
        rows.iter()
            .map(|row| {
                let mut out = row.as_object().cloned().unwrap_or_default();
                let addresses = row["addressList"].as_array().map(Vec::as_slice);
                out.insert(
                    "consensusCount".into(),
                    json!(addresses.map_or(0, <[Value]>::len)),
                );
                out.insert("netInflow".into(), total(addresses, "inflow"));
                out.insert("countBuy".into(), total(addresses, "buyCount"));
                out.insert("countSell".into(), total(addresses, "sellCount"));
                Value::Object(out)
            })
            .collect(),
    ))
}

/// `if(s.toAddressLabel!==void 0&&s.toLabel===void 0){let{toAddressLabel:i,...a}=s;return{...a,toLabel:i}}`
/// — agent mode only; the public endpoint is returned untouched.
fn tx_rows(data: &Value, public: bool) -> Result<Value, String> {
    let rows = data
        .as_array()
        .ok_or("invalid transaction monitor response; expected an array")?;
    Ok(Value::Array(
        rows.iter()
            .map(|row| {
                let mut out = row.as_object().cloned().unwrap_or_default();
                if !public && !out.contains_key("toLabel") {
                    if let Some(label) = out.remove("toAddressLabel") {
                        out.insert("toLabel".into(), label);
                    }
                }
                Value::Object(out)
            })
            .collect(),
    ))
}

fn total(rows: Option<&[Value]>, key: &str) -> Value {
    let sum: f64 = rows
        .map(|rows| rows.iter().map(|row| number(&row[key])).sum())
        .unwrap_or(0.0);
    Number::from_f64(sum).map_or_else(|| json!(0), Value::Number)
}

fn number(value: &Value) -> f64 {
    match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn paged(data: &Value) -> Value {
    if data.is_null() {
        json!({ "total": 0, "rows": [] })
    } else {
        data.clone()
    }
}

/// `i.groupId===o` in the JS readback check; number/string ids are compared by value
/// because the create response and the list rows do not agree on the JSON type.
fn id_matches(group: &Value, id: &Value) -> bool {
    match (group.get("groupId"), id) {
        (Some(Value::Number(a)), Value::Number(b)) => a.as_u64() == b.as_u64(),
        (Some(Value::String(a)), Value::String(b)) => a == b,
        (Some(Value::Number(a)), Value::String(b)) | (Some(Value::String(b)), Value::Number(a)) => {
            b.parse::<u64>().ok() == a.as_u64()
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// validation — every failure is a static, sanitized string and happens before the
// request, so nothing invalid ever reaches the network.
// ---------------------------------------------------------------------------

fn session(client: &Client) -> Result<(), String> {
    // `async function I(){...if(!e)throw ... "No session found, user not logged in"}`
    if client.credentials().session_id.is_empty() {
        Err("wallet session is missing".into())
    } else {
        Ok(())
    }
}

fn chain_id(value: &str) -> Result<&str, String> {
    if CHAINS.contains(&value) {
        Ok(value)
    } else {
        Err(INVALID_CHAIN.into())
    }
}

/// `let o=parseInt(e.groupId,10);isNaN(o)&&T(d.INVALID_PARAMS,\`Invalid group-id: ${e.groupId}\`)`.
/// Only a positive integer is accepted; the JS would also forward `0` or a negative.
fn group_id(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| INVALID_GROUP.to_string())
}

/// `function Ge(t){...return isNaN(e)||e<1||e>kt?null:e}` with `kt=1e4`.
fn page(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|page| (1..=MAX_PAGE).contains(page))
        .ok_or_else(|| "invalid page; expected an integer from 1 to 10000".to_string())
}

/// `function re(t){...return isNaN(e)||e<1||e>nt?null:e}` with `nt=100`, default 20.
fn page_size(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|size| (1..=MAX_PAGE_SIZE).contains(size))
        .ok_or_else(|| "invalid size; expected an integer from 1 to 100".to_string())
}

/// `...e.tokenSize?{tokenSize:parseInt(e.tokenSize,10)}:{}`
fn token_size(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|size| *size > 0)
        .ok_or_else(|| "invalid token-size; expected a positive integer".to_string())
}

/// `function cs(t){let e=t??"24h";return Vl.includes(e)||T(...\`Invalid period\`)...}`
fn period(value: &str) -> Result<&'static str, String> {
    PERIODS
        .iter()
        .copied()
        .find(|period| *period == value)
        .ok_or_else(|| INVALID_PERIOD.to_string())
}

/// `function Mn(t){...if(e==="kol"||e==="1")return 1;if(e==="smy"||e==="smart-money"||e==="2")return 2;
///  T(d.INVALID_PARAMS,\`Invalid tag-type: ${t}. Valid: kol, smy\`)}`
fn tag_type_option(value: &Option<String>) -> Result<Option<u8>, String> {
    value.as_deref().map(tag_type).transpose()
}

fn tag_type(value: &str) -> Result<u8, String> {
    match value.to_ascii_lowercase().as_str() {
        "kol" | "1" => Ok(1),
        "smy" | "smart-money" | "2" => Ok(2),
        _ => Err(INVALID_TAG.into()),
    }
}

/// `function Ar(t){if(!t)return;let e=t.split(",").map(n=>parseInt(n.trim(),10))
///  .filter(n=>!isNaN(n));return e.length>0?e:void 0}` — unparsable elements are
/// dropped exactly like the JS; an empty result is refused instead of silently
/// sending an unfiltered query.
fn trade_sides(value: &str) -> Result<Vec<i64>, String> {
    let sides: Vec<i64> = value
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .collect();
    if sides.is_empty() {
        Err(
            "invalid --trade-side; expected comma-separated integer categories (19, 11, 29, 21)"
                .into(),
        )
    } else {
        Ok(sides)
    }
}

fn usd(value: &str, invalid: &'static str) -> Result<f64, String> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| invalid.to_string())?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(invalid.to_string());
    }
    Ok(parsed)
}

/// The tracker leaves send addresses to the backend unchecked in the JS CLI; an
/// empty, padded or control-character value can only be a mistake, so it is refused
/// here (fail closed) without imposing a per-chain format this crate does not own.
fn address_value(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 128
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(INVALID_ADDRESS.into());
    }
    Ok(value)
}

fn label_value(value: &str) -> Result<&str, String> {
    if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(INVALID_LABEL.into());
    }
    Ok(value)
}

/// `let s=n.name.trim();s||T(...\"Group name cannot be empty\"),s.length>50&&T(...\"Group name
///  exceeds 50 characters\")`. Length is counted in characters, not UTF-16 units.
fn group_name(value: &str) -> Result<&str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("group name cannot be empty".into());
    }
    if trimmed.chars().count() > 50 {
        return Err("group name exceeds 50 characters".into());
    }
    Ok(trimmed)
}

/// `s.split(",").map(l=>l.trim()).filter(Boolean)` for `address/delete`.
fn csv_addresses(value: &str) -> Result<Vec<Value>, String> {
    let addresses: Vec<Value> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| address_value(part).map(|address| json!(address)))
        .collect::<Result<_, _>>()?;
    if addresses.is_empty() {
        return Err(NO_ADDRESSES.into());
    }
    Ok(addresses)
}

/// `--addresses` is either a comma-separated list or a JSON array of `{address,label}`;
/// array entries are forwarded as given after checking they carry an address.
fn batch_addresses(value: &str) -> Result<Vec<Value>, String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('[') {
        // `o=s.split(",").map(l=>l.trim()).filter(Boolean).map(l=>({address:l}))`
        let entries: Vec<Value> = trimmed
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| address_value(part).map(|address| json!({ "address": address })))
            .collect::<Result<_, _>>()?;
        return if entries.is_empty() {
            Err(NO_ADDRESSES.into())
        } else {
            Ok(entries)
        };
    }
    let parsed: Value =
        serde_json::from_str(trimmed).map_err(|_| INVALID_JSON_ADDRESSES.to_string())?;
    let entries = parsed
        .as_array()
        .ok_or_else(|| "--addresses JSON must be an array".to_string())?;
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object().ok_or_else(|| {
            "--addresses JSON entries must be objects with an address".to_string()
        })?;
        let address = object
            .get("address")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "--addresses JSON entries must be objects with an address".to_string()
            })?;
        address_value(address)?;
        out.push(Value::Object(object.clone()));
    }
    if out.is_empty() {
        return Err(NO_ADDRESSES.into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Credentials;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    const EVM: &str = "0x52908400098527886e0f7030069857d2e4169ee7";
    const EVM_B: &str = "0x8617e340b3d01fa5f11f306f4090fd50e238070d";

    fn mock(responses: Vec<String>) -> (String, Receiver<String>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|l| l.parse::<usize>().ok())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (base, rx, thread)
    }

    fn response(data: Value) -> String {
        let body = json!({"code":"000000","data":data}).to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn error_envelope(code: &str) -> String {
        let body = json!({"code":code,"message":"secret-upstream-text","data":null}).to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn credentials() -> Credentials {
        Credentials {
            client_id: zeroize::Zeroizing::new("synthetic-client".into()),
            session_id: zeroize::Zeroizing::new("synthetic-session".into()),
        }
    }

    fn body_of(request: &str) -> Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }

    fn starts_with_post(request: &str, path: &str) -> bool {
        request.starts_with(&format!("POST {path} HTTP/1.1\r\n"))
    }

    // --- args builders -----------------------------------------------------

    fn token_args(group: Option<&str>, tag: Option<&str>) -> TokenArgs {
        TokenArgs {
            chain_id: "56".into(),
            group_id: group.map(str::to_owned),
            tag_type: tag.map(str::to_owned),
            token_size: None,
            period: "24h".into(),
            filter_risk: false,
        }
    }

    fn tx_args(group: Option<&str>) -> TxArgs {
        TxArgs {
            chain_id: "56".into(),
            group_id: group.map(str::to_owned),
            tag_type: None,
            trade_side: None,
            min_value: None,
            max_value: None,
            filter_risk: false,
        }
    }

    fn group_create_args(name: &str) -> GroupCreateArgs {
        GroupCreateArgs {
            chain_id: "56".into(),
            name: name.into(),
        }
    }

    fn group_update_args(group: &str, name: &str) -> GroupUpdateArgs {
        GroupUpdateArgs {
            chain_id: "56".into(),
            group_id: group.into(),
            name: name.into(),
        }
    }

    fn add_args(address: &str) -> AddressAddArgs {
        AddressAddArgs {
            chain_id: "56".into(),
            address: address.into(),
            label: None,
            group_id: None,
            no_overwrite: false,
        }
    }

    fn batch_args(addresses: &str) -> AddressBatchArgs {
        AddressBatchArgs {
            chain_id: "56".into(),
            addresses: addresses.into(),
            group_id: None,
            no_overwrite: false,
        }
    }

    fn delete_args(addresses: &str) -> AddressDeleteArgs {
        AddressDeleteArgs {
            chain_id: "56".into(),
            group_id: "7".into(),
            addresses: addresses.into(),
            yes: false,
        }
    }

    fn follow_args(address: &str) -> AddressFollowArgs {
        AddressFollowArgs {
            chain_id: "56".into(),
            address: address.into(),
            group_id: None,
            label: None,
        }
    }

    fn link_args(address: &str) -> AddressLinkArgs {
        AddressLinkArgs {
            chain_id: "56".into(),
            address: address.into(),
            group_id: None,
            yes: false,
        }
    }

    fn list_args() -> AddressListArgs {
        AddressListArgs {
            chain_id: "56".into(),
            group_id: "7".into(),
            page: "1".into(),
            size: "20".into(),
        }
    }

    fn search_args() -> AddressSearchArgs {
        AddressSearchArgs {
            chain_id: "56".into(),
            group_id: "7".into(),
            address: None,
            label: None,
        }
    }

    fn unfollow_args(address: &str) -> AddressUnfollowArgs {
        AddressUnfollowArgs {
            chain_id: "56".into(),
            address: address.into(),
        }
    }

    fn update_args(address: &str) -> AddressUpdateArgs {
        AddressUpdateArgs {
            chain_id: "56".into(),
            address: address.into(),
            label: "whale".into(),
        }
    }

    // --- one request-shape test per implemented leaf ------------------------

    #[tokio::test]
    async fn token_agent_mode_posts_group_monitor_and_enriches_rows() {
        let (base, requests, handle) = mock(vec![response(json!([{
            "tokenName": "PEPE",
            "ca": "0x6982508145454ce325ddbe47a25d4ec3d2311933",
            "addressList": [
                {"address": EVM, "inflow": 10.5, "buyCount": 2, "sellCount": 1},
                {"address": EVM_B, "inflow": 4.5, "buyCount": 3, "sellCount": 0}
            ]
        }]))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = token(&mut client, &token_args(Some("7"), None))
            .await
            .unwrap();
        let row = &out[0];
        assert_eq!(row["tokenName"], "PEPE");
        assert_eq!(row["consensusCount"], 2);
        assert_eq!(row["netInflow"].as_f64(), Some(15.0));
        assert_eq!(row["countBuy"].as_f64(), Some(5.0));
        assert_eq!(row["countSell"].as_f64(), Some(1.0));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/token/query")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","groupId":7,"period":"24h","enableFilterRiskTokens":false})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn token_public_mode_uses_public_prefix_and_numeric_tag() {
        let (base, requests, handle) = mock(vec![response(json!([]))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = token_args(Some("7"), Some("SMY"));
        args.token_size = Some("5".into());
        args.filter_risk = true;
        let out = token(&mut client, &args).await.unwrap();
        assert_eq!(out, json!([]));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER_PUBLIC}/token/query")
        ));
        // `--group-id` is dropped in public mode, exactly like the JS destructuring.
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","period":"24h","enableFilterRiskTokens":true,"tagType":2,"tokenSize":5})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn tx_agent_mode_posts_filters_and_renames_to_address_label() {
        let (base, requests, handle) = mock(vec![response(json!([{
            "txHash": "0xdead",
            "toAddressLabel": "whale",
            "tradeSideCategory": 19
        }]))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = tx_args(Some("7"));
        args.trade_side = Some("19, 11,nonsense".into());
        args.min_value = Some("100".into());
        args.max_value = Some("2500.5".into());
        args.filter_risk = true;
        let out = tx(&mut client, &args).await.unwrap();
        assert_eq!(out[0]["toLabel"], "whale");
        assert!(out[0].get("toAddressLabel").is_none());
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/query")
        ));
        let body = body_of(&request);
        assert_eq!(body["chainId"], "56");
        assert_eq!(body["groupId"], 7);
        assert_eq!(body["tradeSideCategory"], json!([19, 11]));
        assert_eq!(body["minTxValue"].as_f64(), Some(100.0));
        assert_eq!(body["maxTxValue"].as_f64(), Some(2500.5));
        assert_eq!(body["enableFilterRiskTokens"], true);
        assert!(body.get("tagType").is_none());
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn tx_public_mode_uses_public_prefix_and_keeps_group() {
        let (base, requests, handle) = mock(vec![response(json!([{
            "toAddressLabel": "not-renamed-in-public-mode"
        }]))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = tx_args(Some("7"));
        args.tag_type = Some("kol".into());
        let out = tx(&mut client, &args).await.unwrap();
        assert_eq!(out[0]["toAddressLabel"], "not-renamed-in-public-mode");
        assert!(out[0].get("toLabel").is_none());
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER_PUBLIC}/address/query")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","groupId":7,"enableFilterRiskTokens":false,"tagType":1})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn follow_posts_chain_and_returns_followings_losslessly() {
        let (base, requests, handle) = mock(vec![response(json!({
            EVM: {"label": "whale", "follow": true}
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = followings(
            &mut client,
            &FollowArgs {
                chain_id: "1".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(out[EVM]["label"], "whale");
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/followings")
        ));
        assert_eq!(body_of(&request), json!({"chainId":"1"}));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn group_list_posts_include_all_group_both_ways() {
        for (no_all_group, expected) in [(false, true), (true, false)] {
            let (base, requests, handle) = mock(vec![response(json!([
                {"groupId": 1, "groupName": "All", "addressCount": 3, "displayOrder": 0}
            ]))]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            let out = group_list(
                &mut client,
                &GroupListArgs {
                    chain_id: "56".into(),
                    no_all_group,
                },
            )
            .await
            .unwrap();
            assert_eq!(out[0]["groupName"], "All");
            let request = requests.recv().unwrap();
            assert!(starts_with_post(&request, &format!("{TRACKER}/group/list")));
            assert_eq!(
                body_of(&request),
                json!({"chainId":"56","includeAllGroup":expected})
            );
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn group_create_posts_name_and_confirms_on_readback() {
        let (base, requests, handle) = mock(vec![
            response(json!(42)),
            response(json!([{"groupId": 42, "groupName": "whales"}])),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = group_create(&mut client, &group_create_args("  whales  "))
            .await
            .unwrap();
        // `g({groupId:a,groupName:n.name})` keeps the untrimmed argv name.
        assert_eq!(out, json!({"groupId": 42, "groupName": "  whales  "}));
        let create = requests.recv().unwrap();
        assert!(starts_with_post(
            &create,
            &format!("{TRACKER}/group/create")
        ));
        assert_eq!(
            body_of(&create),
            json!({"chainId":"56","groupName":"whales"})
        );
        let readback = requests.recv().unwrap();
        assert!(starts_with_post(
            &readback,
            &format!("{TRACKER}/group/list")
        ));
        assert_eq!(
            body_of(&readback),
            json!({"chainId":"56","includeAllGroup":true})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn group_create_without_readback_confirmation_fails() {
        let (base, requests, handle) = mock(vec![
            response(json!(42)),
            response(json!([{"groupId": 7, "groupName": "other"}])),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            group_create(&mut client, &group_create_args("whales"))
                .await
                .unwrap_err(),
            "group create not confirmed on readback"
        );
        requests.recv().unwrap();
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn group_update_posts_id_and_name_and_confirms_on_readback() {
        let (base, requests, handle) = mock(vec![
            response(json!(true)),
            response(json!([{"groupId": 7, "groupName": "whales"}])),
        ]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = group_update(&mut client, &group_update_args("7", "whales"))
            .await
            .unwrap();
        assert_eq!(out, json!({"groupId": 7, "groupName": "whales"}));
        let update = requests.recv().unwrap();
        assert!(starts_with_post(
            &update,
            &format!("{TRACKER}/group/update")
        ));
        assert_eq!(
            body_of(&update),
            json!({"chainId":"56","groupId":7,"groupName":"whales"})
        );
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_add_posts_import_envelope() {
        let (base, requests, handle) = mock(vec![response(json!(1))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = add_args(EVM);
        args.label = Some("whale".into());
        args.group_id = Some("7".into());
        args.no_overwrite = true;
        let out = address_add(&mut client, &args).await.unwrap();
        assert_eq!(out, json!({"imported":1}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/import")
        ));
        assert_eq!(
            body_of(&request),
            json!({
                "chainId":"56",
                "allowOverwrite":false,
                "addressList":[{"address":EVM,"label":"whale"}],
                "groupId":7
            })
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_batch_posts_csv_and_json_lists() {
        for (input, expected_list, expected_count) in [
            (
                format!("{EVM}, {EVM_B}"),
                json!([{"address":EVM},{"address":EVM_B}]),
                2,
            ),
            (
                format!(r#"[{{"address":"{EVM}","label":"a"}}]"#),
                json!([{"address":EVM,"label":"a"}]),
                1,
            ),
        ] {
            let (base, requests, handle) = mock(vec![response(json!(2))]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            let out = address_batch(&mut client, &batch_args(&input))
                .await
                .unwrap();
            assert_eq!(
                out,
                json!({"imported":2,"count":expected_count,"invalidAddresses":[]})
            );
            let request = requests.recv().unwrap();
            assert!(starts_with_post(
                &request,
                &format!("{TRACKER}/address/import")
            ));
            assert_eq!(
                body_of(&request),
                json!({
                    "chainId":"56",
                    "allowOverwrite":true,
                    "addressList":expected_list
                })
            );
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn address_delete_posts_group_and_address_list() {
        let (base, requests, handle) = mock(vec![response(json!(true))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = address_delete(&mut client, &delete_args(&format!("{EVM}, {EVM_B}")))
            .await
            .unwrap();
        assert_eq!(out, json!({"groupId":7,"deleted":[EVM,EVM_B]}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/delete")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","groupId":7,"addressList":[EVM,EVM_B]})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_follow_posts_group_and_label() {
        let (base, requests, handle) = mock(vec![response(json!(true))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = follow_args(EVM);
        args.group_id = Some("7".into());
        args.label = Some("whale".into());
        let out = address_follow(&mut client, &args).await.unwrap();
        assert_eq!(out, json!({"address":EVM,"followed":true}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/follow")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","address":EVM,"groupId":7,"label":"whale"})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_link_posts_target_group() {
        for (group, expected) in [
            (None, json!({"chainId":"56","address":EVM})),
            (Some("7"), json!({"chainId":"56","address":EVM,"groupId":7})),
        ] {
            let (base, requests, handle) = mock(vec![response(json!(true))]);
            let mut client = Client::test_client(credentials(), &base).unwrap();
            let mut args = link_args(EVM);
            args.group_id = group.map(str::to_owned);
            let out = address_link(&mut client, &args).await.unwrap();
            let mut expected_out = json!({"address":EVM});
            if let Some(group) = group {
                expected_out["groupId"] = json!(group.parse::<u64>().unwrap());
            }
            assert_eq!(out, expected_out);
            let request = requests.recv().unwrap();
            assert!(starts_with_post(
                &request,
                &format!("{TRACKER}/address/group/link")
            ));
            assert_eq!(body_of(&request), expected);
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn address_list_posts_page_and_size() {
        let (base, requests, handle) = mock(vec![response(json!({
            "total": 2,
            "rows": [{"address": EVM, "label": "whale", "isFollowed": true}]
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = list_args();
        args.page = "3".into();
        args.size = "50".into();
        let out = address_list(&mut client, &args).await.unwrap();
        assert_eq!(out["total"], 2);
        assert_eq!(out["rows"][0]["address"], EVM);
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/page")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","groupId":7,"page":3,"pageSize":50})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_search_posts_fuzzy_filters() {
        let (base, requests, handle) = mock(vec![response(json!({
            "total": 0,
            "rows": []
        }))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let mut args = search_args();
        args.address = Some("0x5290".into());
        args.label = Some("whal".into());
        let out = address_search(&mut client, &args).await.unwrap();
        assert_eq!(out, json!({"total":0,"rows":[]}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/search")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","groupId":7,"address":"0x5290","label":"whal"})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_unfollow_posts_address() {
        let (base, requests, handle) = mock(vec![response(json!(true))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = address_unfollow(&mut client, &unfollow_args(EVM))
            .await
            .unwrap();
        assert_eq!(out, json!({"address":EVM,"followed":false}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/unfollow")
        ));
        assert_eq!(body_of(&request), json!({"chainId":"56","address":EVM}));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_update_prefers_address_update_and_falls_back_to_label_update() {
        // First endpoint succeeds: a single request.
        let (base, requests, handle) = mock(vec![response(json!(true))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = address_update(&mut client, &update_args(EVM))
            .await
            .unwrap();
        assert_eq!(out, json!({"address":EVM,"label":"whale"}));
        let request = requests.recv().unwrap();
        assert!(starts_with_post(
            &request,
            &format!("{TRACKER}/address/update")
        ));
        assert_eq!(
            body_of(&request),
            json!({"chainId":"56","address":EVM,"label":"whale"})
        );
        handle.join().unwrap();

        // First endpoint fails, the second succeeds: reported as success.
        let (base, requests, handle) = mock(vec![error_envelope("1200009"), response(json!(true))]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let out = address_update(&mut client, &update_args(EVM))
            .await
            .unwrap();
        assert_eq!(out, json!({"address":EVM,"label":"whale"}));
        requests.recv().unwrap();
        let fallback = requests.recv().unwrap();
        assert!(starts_with_post(
            &fallback,
            &format!("{TRACKER}/address/label/update")
        ));
        assert_eq!(
            body_of(&fallback),
            json!({"chainId":"56","address":EVM,"label":"whale"})
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn address_update_failing_both_endpoints_is_a_static_error() {
        let (base, requests, handle) =
            mock(vec![error_envelope("1200009"), error_envelope("1200010")]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let error = address_update(&mut client, &update_args(EVM))
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "address update failed on both address/update and address/label/update"
        );
        assert!(!error.contains("secret"));
        requests.recv().unwrap();
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn ws_is_explicitly_unimplemented_and_never_reaches_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &base).unwrap();
        assert_eq!(
            run(&mut client, &TrackerCommand::Ws).await.unwrap_err(),
            "tracker ws requires websocket support, not implemented"
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    // --- classification, journal and error handling -------------------------

    #[test]
    fn mutation_classification_covers_every_leaf() {
        let reads = [
            TrackerCommand::Token(token_args(Some("7"), None)),
            TrackerCommand::Tx(tx_args(Some("7"))),
            TrackerCommand::Follow(FollowArgs {
                chain_id: "56".into(),
            }),
            TrackerCommand::Group {
                command: GroupCommand::List(GroupListArgs {
                    chain_id: "56".into(),
                    no_all_group: false,
                }),
            },
            TrackerCommand::Address {
                command: AddressCommand::List(list_args()),
            },
            TrackerCommand::Address {
                command: AddressCommand::Search(search_args()),
            },
            TrackerCommand::Ws,
        ];
        for read in &reads {
            assert!(!is_mutation(read), "{read:?} must be a read");
            assert_eq!(mutation_payload(read), Err("not a mutation".into()));
        }

        let writes = [
            TrackerCommand::Group {
                command: GroupCommand::Create(group_create_args("whales")),
            },
            TrackerCommand::Group {
                command: GroupCommand::Update(group_update_args("7", "whales")),
            },
            TrackerCommand::Address {
                command: AddressCommand::Add(add_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Batch(batch_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Delete(delete_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Follow(follow_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Link(link_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Unfollow(unfollow_args(EVM)),
            },
            TrackerCommand::Address {
                command: AddressCommand::Update(update_args(EVM)),
            },
        ];
        // 7 reads + 9 writes = 16 leaves, so nothing is classified implicitly.
        // (The task brief counts 15 leaves including `ws`; the enumerated family —
        // token, tx, follow, group create/list/update, the nine address leaves and
        // ws — is 16, and every one of them is asserted here.)
        assert_eq!(reads.len() + writes.len(), 16);
        for write in &writes {
            assert!(is_mutation(write), "{write:?} must be a mutation");
            let first = mutation_payload(write).unwrap();
            assert_eq!(
                first,
                mutation_payload(write).unwrap(),
                "must be deterministic"
            );
            assert_eq!(first["args"]["chainId"], "56");
            assert!(!first.to_string().contains("synthetic-session"));
        }
        assert_eq!(
            mutation_payload(&TrackerCommand::Address {
                command: AddressCommand::Delete(delete_args(EVM)),
            })
            .unwrap(),
            json!({
                "operation": "tracker.address.delete",
                "args": {"chainId":"56","groupId":7,"addressList":[EVM]}
            })
        );
    }

    #[tokio::test]
    async fn backend_error_envelope_is_sanitized() {
        let (base, requests, handle) = mock(vec![error_envelope("1200009")]);
        let mut client = Client::test_client(credentials(), &base).unwrap();
        let error = token(&mut client, &token_args(Some("7"), None))
            .await
            .unwrap_err();
        assert_eq!(error, "Binance API code 1200009");
        assert!(!error.contains("secret"));
        requests.recv().unwrap();
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn missing_or_invalid_options_never_reach_the_network() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut client = Client::test_client(credentials(), &base).unwrap();

        let mut no_group = token_args(None, None);
        no_group.group_id = None;
        assert!(run(&mut client, &TrackerCommand::Token(no_group))
            .await
            .is_err());
        // ... and an invalid chain, group, page, size or address.
        let mut bad_chain = token_args(Some("7"), None);
        bad_chain.chain_id = "999".into();
        assert!(run(&mut client, &TrackerCommand::Token(bad_chain))
            .await
            .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Tx(TxArgs {
                chain_id: String::new(),
                ..tx_args(Some("7"))
            })
        )
        .await
        .is_err());
        let mut bad_group = list_args();
        bad_group.group_id = "abc".into();
        assert!(run(
            &mut client,
            &TrackerCommand::Address {
                command: AddressCommand::List(bad_group)
            }
        )
        .await
        .is_err());
        let mut bad_page = list_args();
        bad_page.page = "0".into();
        assert!(address_list(&mut client, &bad_page).await.is_err());
        let mut bad_size = list_args();
        bad_size.size = "101".into();
        assert!(address_list(&mut client, &bad_size).await.is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Address {
                command: AddressCommand::Add(add_args("not an address"))
            }
        )
        .await
        .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Address {
                command: AddressCommand::Batch(batch_args("[{\"label\":\"no address\"}]"))
            }
        )
        .await
        .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Address {
                command: AddressCommand::Batch(batch_args("[broken"))
            }
        )
        .await
        .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Address {
                command: AddressCommand::Delete(delete_args(" , "))
            }
        )
        .await
        .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Group {
                command: GroupCommand::Create(group_create_args("   "))
            }
        )
        .await
        .is_err());
        assert!(run(
            &mut client,
            &TrackerCommand::Group {
                command: GroupCommand::Create(group_create_args(&"x".repeat(51)))
            }
        )
        .await
        .is_err());
        let mut bad_period = token_args(Some("7"), None);
        bad_period.period = "2h".into();
        assert!(token(&mut client, &bad_period).await.is_err());
        let mut bad_tag = tx_args(Some("7"));
        bad_tag.tag_type = Some("influencer".into());
        assert!(tx(&mut client, &bad_tag).await.is_err());
        let mut bad_sides = tx_args(Some("7"));
        bad_sides.trade_side = Some("buy,sell".into());
        assert!(tx(&mut client, &bad_sides).await.is_err());
        let mut bad_value = tx_args(Some("7"));
        bad_value.min_value = Some("-1".into());
        assert!(tx(&mut client, &bad_value).await.is_err());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn clap_surface_matches_the_installed_help() {
        use crate::cli::{Cli, Command};
        use clap::Parser;

        let argv = [
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "address",
            "add",
            "-c",
            "56",
            "-a",
            EVM,
            "-l",
            "whale",
            "-g",
            "7",
            "--no-overwrite",
        ];
        match Cli::try_parse_from(argv).unwrap().command {
            Command::Tracker {
                command:
                    TrackerCommand::Address {
                        command: AddressCommand::Add(args),
                    },
            } => {
                assert_eq!(args.chain_id, "56");
                assert_eq!(args.address, EVM);
                assert_eq!(args.label.as_deref(), Some("whale"));
                assert_eq!(args.group_id.as_deref(), Some("7"));
                assert!(args.no_overwrite);
            }
            other => panic!("unexpected grammar: {other:?}"),
        }

        // `--period` and `--page`/`--size` defaults come from the installed CLI.
        let token_cli = Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "token",
            "-c",
            "56",
            "-g",
            "7",
        ])
        .unwrap();
        match token_cli.command {
            Command::Tracker {
                command: TrackerCommand::Token(args),
            } => assert_eq!(args.period, "24h"),
            other => panic!("unexpected grammar: {other:?}"),
        }
        let list_cli = Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "address",
            "list",
            "-c",
            "56",
            "-g",
            "7",
        ])
        .unwrap();
        match list_cli.command {
            Command::Tracker {
                command:
                    TrackerCommand::Address {
                        command: AddressCommand::List(args),
                    },
            } => {
                assert_eq!(args.page, "1");
                assert_eq!(args.size, "20");
            }
            other => panic!("unexpected grammar: {other:?}"),
        }

        // One parse per leaf, with every flagged option set, so no leaf has a
        // conflicting or missing flag in the landed grammar.
        let base = [
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
        ];
        let leaves: [&[&str]; 15] = [
            &[
                "tracker",
                "token",
                "-c",
                "56",
                "-g",
                "7",
                "--tag-type",
                "kol",
                "--token-size",
                "70",
                "--period",
                "1h",
                "--filter-risk",
            ],
            &[
                "tracker",
                "tx",
                "-c",
                "56",
                "-g",
                "7",
                "--tag-type",
                "smy",
                "--trade-side",
                "19,11",
                "--min-value",
                "1",
                "--max-value",
                "2",
                "--filter-risk",
            ],
            &["tracker", "follow", "-c", "56"],
            &["tracker", "group", "create", "-c", "56", "-n", "whales"],
            &["tracker", "group", "list", "-c", "56", "--no-all-group"],
            &[
                "tracker", "group", "update", "-c", "56", "-g", "7", "-n", "whales",
            ],
            &[
                "tracker",
                "address",
                "add",
                "-c",
                "56",
                "-a",
                EVM,
                "-l",
                "whale",
                "-g",
                "7",
                "--no-overwrite",
            ],
            &[
                "tracker",
                "address",
                "batch",
                "-c",
                "56",
                "-a",
                EVM,
                "-g",
                "7",
                "--no-overwrite",
            ],
            &[
                "tracker", "address", "delete", "-c", "56", "-g", "7", "-a", EVM, "-y",
            ],
            &[
                "tracker", "address", "follow", "-c", "56", "-a", EVM, "-g", "7", "-l", "whale",
            ],
            &[
                "tracker", "address", "link", "-c", "56", "-a", EVM, "-g", "7", "-y",
            ],
            &[
                "tracker", "address", "list", "-c", "56", "-g", "7", "--page", "2", "--size", "10",
            ],
            &[
                "tracker", "address", "search", "-c", "56", "-g", "7", "-a", "0x5290", "-l",
                "whale",
            ],
            &["tracker", "address", "unfollow", "-c", "56", "-a", EVM],
            &[
                "tracker", "address", "update", "-c", "56", "-a", EVM, "-l", "whale",
            ],
        ];
        for leaf in leaves {
            let argv: Vec<&str> = base.iter().copied().chain(leaf.iter().copied()).collect();
            Cli::try_parse_from(argv).unwrap_or_else(|error| panic!("{leaf:?}: {error}"));
        }

        // `requiredOption("-c, --chain-id <chainId>")` on every tracker leaf.
        assert!(Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "token",
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "group",
            "create",
            "-c",
            "56",
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "address",
            "update",
            "-c",
            "56",
            "-a",
            EVM,
        ])
        .is_err());

        // `ws` stays a parseable leaf even though the transport is not implemented.
        assert!(Cli::try_parse_from([
            "baw",
            "--wallet-dir",
            "/tmp/wallet",
            "--instance-id",
            "instance",
            "tracker",
            "ws",
        ])
        .is_ok());
    }
}
