# Native market orders and wallet transfers

Implemented in `src/trade.rs`, based on the installed 1.12.0 bundle and the offline sanitized API specification. Development/tests use synthetic credentials and loopback TCP only. No live session was opened or wallet operation authorized.

## Public integration contract

```rust
pub enum MarketCommand { Quote(QuoteArgs), Swap(SwapArgs), List(ListArgs) } // clap::Subcommand
pub struct QuoteArgs { // clap::Args, Clone, Serialize
    pub binance_chain_id: String, // --binanceChainId
    pub from_token: String,       // --fromToken
    pub to_token: String,         // --toToken
    pub from_token_qty: String,   // --fromTokenQty
    pub slippage: String,         // --slippage; default "auto"
}
pub struct SwapArgs { // clap::Args; QuoteArgs flattened in clap and JSON
    pub quote: QuoteArgs,
    pub mev: String,                  // --mev; default "true"
    pub gas_level: String,            // --gasLevel; default "MEDIUM"
    pub acknowledge_token_risk: bool, // --acknowledge-token-risk
}
pub struct ListArgs { // clap::Args; Default uses page=1, page_size=20
    pub order_id: Option<String>,         // --orderId
    pub binance_chain_id: Option<String>, // --binanceChainId
    pub status: Option<String>,           // --status
    pub from_token: Option<String>,       // --fromToken
    pub to_token: Option<String>,         // --toToken
    pub start_time: Option<String>,       // --startTime; millisecond integer
    pub end_time: Option<String>,         // --endTime
    pub page: u32,                        // --page
    pub page_size: u32,                   // --pageSize
}
pub struct SendArgs { // clap::Args, Clone, Serialize
    pub binance_chain_id: String, // --binanceChainId
    pub amount: Option<String>,   // --amount
    pub max: bool,                // --max; exactly one of amount/max
    pub recipient: String,       // --recipient; address, not alias/domain
    pub token_address: String,   // --tokenAddress
    pub gas_level: String,       // --gasLevel; default "MEDIUM"
}
pub async fn run(client: &mut Client, command: &MarketCommand) -> Result<Value, String>;
pub async fn send(client: &mut Client, args: &SendArgs) -> Result<Value, String>;
pub fn is_mutating(command: &MarketCommand) -> bool;
pub fn mutation_payload(command: &MarketCommand) -> Result<Value, String>;
pub fn send_payload(args: &SendArgs) -> Result<Value, String>;
```

The parent owns explicit confirmation, persistent operation IDs, durable anti-replay journaling and serialization **before** calling `run(Swap)` or `send`. These functions do not duplicate that guard. Intent functions are pure, validate input, and produce `{"operation":"market-order.swap"|"wallet.send","args":{...}}`. They preserve original amount strings, requested gas spelling, slippage percent, MEV and risk acknowledgement, rather than journaling a silently converted API body. Serialization uses camelCase and flattens `SwapArgs.quote`.

## Validation and protections

- Amounts are bounded unsigned decimal strings (`digits[.digits]`, positive, at most 1e18), never parsed with floats. No whitespace, exponent syntax, signs, empty fraction or trailing junk. Up to 512 characters; this deliberate bound prevents unbounded allocation. Decimal percent slippage is exact: `1.234567891234567891` → `0.01234567891234567891`; range 0..100, default `auto`.
- Supported chain IDs follow the original actual validation set: `1`, `56`, `8453`, `CT_501`, `4663`. Original help mentions `5042`, but its validator does not accept it. EVM tokens/recipients require 0x plus 40 hex digits: all-lowercase accepted, any other casing must match the parent's Keccak EIP55 checksum. Solana uses original base58 length 32..44 validation, with case-sensitive matching; no claim of full public-key decoding.
- Gas levels LOW/MEDIUM/HIGH are case-insensitive; actual request uppercases them. MEV requires the literal `true` or `false`.
- Normal/bstock/unknown-RWA swaps support MEV on `1`, `56`, `CT_501`. Unlike the original, unsupported chains do **not silently force false**: explicitly request `--mev false`.
- Ondo endpoints accept neither gas selection nor MEV fields. Require `--mev false` and the default MEDIUM gas setting; HIGH/LOW fail rather than silently ignoring the selection. MEDIUM is an unset/default sentinel on that endpoint, not a promise of a selected gas fee.
- Every non-native target requires `--acknowledge-token-risk`, including registry-listed RWA tokens (a registry is not a security audit). Native sentinels are EVM zero/eeee and Solana wrapped SOL. There is no local trusted-token allowlist or token-audit integration, and the flag **never represents a successful audit or overrides backend policy**. The installed original does not automatically audit tokens either. Quote is read-only and does not require this acceptance.
- Swap reads the RWA registry, settings and daily quota, requires quota remaining, rejects developer-mode balance-exceeded/expired or unknown enabled-mode expiry, then requires exact `status: UNLOCKED` from tx-lock. Missing/malformed settings or lock state fails closed. Backend policies remain authoritative. These local quota checks detect exhaustion, not a guaranteed USD cost or gas cap: backend admission must still enforce the actual trade's cost and token restrictions.
- Swap obtains a fresh native quote before submission; requires quote ID and positive output, rejects nested quote errors, and for explicit slippage requires quote slippage no greater than the requested fraction. Order still sends the requested slippage (`auto` remains auto), never disables it. This is a preflight, **not** a quote-ID-bound execution guarantee: original place-order API has no quoteId field. No order polling or automatic resubmission.
- List: status PENDING/FINISHED/FAILED, page 1..10000, pageSize 1..100, orderId alphanumeric/hyphen up to 64. Times are digit-only millisecond integers between January 1, 2020 and current time plus 100×365 days, with start≤end. All filters validate even when orderId overrides them. OrderId request body is only `{orderId,page:1,pageSize:1}`.

## Exact RWA compatibility

All market commands fetch the command-local registry (even order-ID list). Normal tokens use `/web-dex/agent/quote` and `/place-order`. RWA context selects the source token first, otherwise destination, matching original precedence even when both are RWA.

| Kind | Quote | Order |
|---|---|---|
| type 1 / Ondo | `/ondo/quote`, `fromAmount`, `fromTokenShare` / `fromMultiplier` | `/ondo/place-order`, `amount`, `tokenShare` / `multiplier`; no gas/MEV fields |
| type 3 / bstock | ordinary quote, `amount`, plus share/multiplier fields | ordinary order plus share/multiplier fields |
| unknown RWA type | same as bstock | same as bstock |

Sell quote divides requested shares by multiplier, rounding **UP at 18 decimal places**. Sell order/send reads the raw wallet token row, divides UP at the token's own decimal precision (original behavior, commonly 18), then clamps the rounded base amount to raw wallet balance. Before clamping, exact cross multiplication rejects true oversell; no rounded comparison hides oversell. Multiplier 1 preserves the input string, while still rejecting oversell (an intentional tightening of the original multiplier-1 shortcut). Matching respects Solana case and treats EVM addresses case-insensitively.

Buy quote multiplies underlying output by destination multiplier and rounds DOWN18. Ondo prefers `toTokenShare`, otherwise backend `toMultiplier`, otherwise registry multiplier. Normal RWA buy quote uses registry multiplier. Sell output restores original share amount as `fromCoinAmount`. Share/multiplier request fields are explicit null on buy; absent on non-RWA normal requests.

## Transfer policy and results

`--amount` XOR `--max` is enforced both by clap and public-call validation. Explicit amount follows original registry/share conversion; max omits amount and sends `isMax:true` without fetching registry/balance. Transfer sends exactly chain, `recipientAddress`, token, **`feeLevel`** (not gasLevel), plus amount or isMax. It does not create/address-book entries, resolve aliases, bypass recipient allowlists, or auto-confirm in the app. Backend recipient/address-book rules and confirmation are preserved. `PENDING_CONFIRMATION` is surfaced as APP_CONFIRMATION_REQUIRED without claiming sent/confirmed.

Successful order acceptance returns `status: SUBMITTED`, orderId/clientOrderId and retained quote. It never infers FILLED from an order ID. Transfer with a txHash similarly returns SUBMITTED with raw `backendData`; on-chain confirmation must be queried independently. Nested order `data.code != "000000"` returns ORDER_API_ERROR with a bounded numeric code, static sanitized message and optional orderId/orderExpireTime. Backend error message text is deliberately not echoed; transfer inner failure codes also reject rather than reporting SUBMITTED.

No application-level retries, polling or automatic resubmission exist. Once an execute request is attempted, transport/envelope errors (including timeouts), missing IDs or unusable response shapes return `status: UNCERTAIN`, `retrySafe:false`; reconcile order/transaction history manually. This conservatively includes backend rejection envelopes when the shared client exposes only an undifferentiated error string. The parent's durable guard must keep the operation consumed even when uncertain or failed.

## Deliberate output improvements

Quote retains quoteId, feeDetail, gasDetails and any new backend metadata instead of the installed original JSON command's information loss. Amount presentation alone is RWA-normalized. List retains original order mappings: chain fallback, checksummed token addresses, **toTokenActualQty** rather than a quoted quantity, unchanged backend status, orderTxId→txHash, local-offset ISO dates, and parsed refund entries with RWA conversion. Malformed `extend` JSON is ignored, as in the original. Rows remain in server order; no claimed submitted=filled mapping. Registry cache behavior is owned/documented by the portfolio module.

## Examples (illustrative; NOT executed)

```sh
baw-rs market-order quote --binanceChainId 56 \
  --fromToken 0x1111111111111111111111111111111111111111 \
  --toToken 0x2222222222222222222222222222222222222222 --fromTokenQty 1.00 --slippage 1
baw-rs market-order list --binanceChainId 56 --status PENDING --page 1 --pageSize 20
# Mutation additionally requires the parent/global confirmation and unique operation-ID flags:
baw-rs market-order swap --binanceChainId 56 \
  --fromToken 0x1111111111111111111111111111111111111111 \
  --toToken 0x2222222222222222222222222222222222222222 --fromTokenQty 1.00 \
  --slippage 1 --acknowledge-token-risk
# For an Ondo token: additionally --mev false (leave gasLevel at MEDIUM).
baw-rs wallet send --binanceChainId 56 \
  --recipient 0x2222222222222222222222222222222222222222 \
  --tokenAddress 0x1111111111111111111111111111111111111111 --amount 1.00 --gasLevel HIGH
# Or replace --amount with --max, never both.
```

The example tokens are synthetic placeholders, not recommendations or verified assets. See scratch `trade-*-red.log` / `trade-*-green.log` for vertical TDD evidence and the isolated harness for test commands. The harness has its own Cargo target directory; it must never share root build artifacts.
