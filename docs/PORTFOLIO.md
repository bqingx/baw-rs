# Portfolio and RWA port

## Public integration

`portfolio::balance(&mut Client, &BalanceArgs)`, `portfolio::history(&mut Client, &HistoryArgs)` and `portfolio::left_quota(&mut Client)` return `Result<serde_json::Value, String>`.

Parent CLI integration should flatten the public clap `Args` types:

- `BalanceArgs`: `raw: bool`, `symbol: Option<String>`, `token_address: Option<String>`, `binance_chain_id: Option<String>`, `hide_low_value: Option<String>`.
- `HistoryArgs`: `history_type: String` (CLI `--type`, default `all`), `binance_chain_id: Option<String>`, `size: u16` (default 20), `next_cursor: Option<String>`, `tx: Option<String>`, `start_time: Option<String>`, `end_time: Option<String>`.
- The camel-case option spellings match the installed client: `--tokenAddress`, `--binanceChainId`, `--hideLowValue`, `--nextCursor`, `--startTime`, `--endTime`.
- `left-quota` takes no arguments; no `QuotaArgs` is necessary.

`rwa::Registry: Clone` provides `token(chain, address) -> Option<&RwaToken>`. The public token fields are exactly `multiplier: String`, `kind: String`, and `rwa_type: Option<i64>`. `rwa::fetch(&mut Client)` fetches a fresh command-local registry. `multiply_down(amount, multiplier, scale)` truncates multiplication toward zero; `divide_up(amount, multiplier, scale)` rounds division away from zero. Both return plain decimal strings or an error.

## Requests and scope

Only `Client.request` is used. There is no wallet executable, signing, execute endpoint, persistent cache, credential loading, session-file update, or background polling in these modules.

| Operation | Read-only HTTP request |
|---|---|
| Balance | bodyless POST `/bapi/defi/v1/public/wallet-direct/agent-wallet/mpc-wallet/token/list` |
| RWA registry | GET `/bapi/defi/v1/public/wallet-direct/buw/wallet/market/token/rwa/stock/scaleui/list` |
| Pending history | GET `/bapi/defi/v2/public/wallet-direct/agent/tx-history-pending` |
| Confirmed history | GET `/bapi/defi/v2/public/wallet-direct/agent/tx-history-confirm` |
| Single transaction | GET `/bapi/defi/v2/public/wallet-direct/agent/tx-status` |
| Quota | GET agent-wallet `/trading-limit/query`, then `/settings/query` |

Queries use `url::form_urlencoded`, including encoding literal `+`, `/`, `=` in backend pagination tokens. Sequential requests are intentional because the shared client carries session rotation in memory.

## Balance

`--raw` preserves the existing raw balance mode: return `data.tokenList`, or `[]` when missing/null, without fetching a registry, converting amounts, normalizing/checksumming addresses, filtering rows, or stripping backend fields. Supplied options are still validated before requests. Filters affect normalized mode only.

Normalized mode fetches the token list and registry once, then emits `{symbol?, address, binanceChainId?, balance, price, value}`. Backend balance strings must parse as finite decimals; invalid, negative, and zero balances are omitted. Missing/empty prices become `"0"`; invalid non-RWA prices have zero numeric value, as with the installed client's fallback. Balance, price, and USD value are strings, computed with arbitrary-precision decimals rather than binary floats.

For every registered RWA (including unknown types), balance is `rawBalance * multiplier`, truncated to 18 decimal places, and price is `rawPrice / multiplier`, truncated to 18 places. The output value is the exact product of those displayed values. Type 1 is `ondo`, type 3 is `bstock`, and other/missing types are `unknown`. JSON does not expose the installed client's internal RWA display metadata.

Default low-value hiding removes values below exactly `0.01`; `--tokenAddress` disables it unless explicitly overridden with `--hideLowValue true`. Zero balances remain hidden regardless. Symbol matching is ASCII case-insensitive. Chain matching is exact. EVM token addresses are case-insensitive; Solana (`CT_501`) token identifiers are case-sensitive. Registry keys include the exact chain, so identical addresses on different chains do not collide.

Registry rows require nonempty string chain/address and a string multiplier that parses as finite and strictly positive. Invalid rows are ignored rather than poisoning the command. A non-array registry response fails explicitly. There is no global, on-disk, stale, or cross-command registry cache.

## Transaction history

`--type pending` and `--type confirmed` call only their respective history endpoint after the registry query, with raw backend pagination cursors. Requests include `hideRisk=true`, `hideLowValue=true`, `size`, default `startTime = now - exactly 90 days`, optional `endTime`, optional plural `binanceChainIds`, and optional `nextToken`.

All-mode reproduces the installed state machine:

1. A cursor beginning with the exact marker `__CONFIRMED__:` skips pending and requests confirmed only. An empty suffix starts confirmed history without a `nextToken` query parameter.
2. Otherwise fetch pending first. If pending fills the page, emit its nonempty next token unchanged, or the synthetic `__CONFIRMED__:` cursor when exhausted. `hasMore` is true even when the next confirmed page may be empty.
3. If pending underfills, request confirmed with the remaining page size and **no cursor**, concatenate pending before confirmed, and prefix a nonempty confirmed cursor with `__CONFIRMED__:`.

There is no global timestamp merge and no automatic page draining. The installed underfilled-pending behavior discards a pending continuation even if the backend supplied one; this is retained for compatibility and documented as a backend-dependent limitation. Oversized pending pages are truncated to the requested size, matching the installed client.

History rows project transaction type/hash/time/chain/status and the restricted nested transaction hashes, fee metadata, send/receive/approve instructions. Backend statuses remain unchanged in list history, including unknown statuses and explicit nulls. Optional projected fields are not filled with invented nulls. Send/receive integer amounts are converted exactly from base units with `BigInt`, including amounts beyond `u64`, then multiplied by the RWA multiplier when a chain-specific token is known. Approval amounts and network-fee raw amounts remain unchanged. Missing chain information never triggers a guessed cross-chain RWA conversion. Malformed/unconvertible instruction amounts retain their backend value.

`--tx` requests only `/tx-status?txHash=...&binanceChainId=...`; it never loads a registry or sends date/default pagination parameters. Null backend data returns null. Its status is separately normalized: `confirmed`/`partial_success` → `SUCCESS`; `submit_fail`/`expire_fail`/`dropped`/`failed`/`reverted`/`replaced` → `FAILED`; everything else → `PENDING`.

Millisecond timestamps become local RFC3339 ISO strings with an explicit numeric timezone offset and whole-second precision, matching installed formatting. Invalid/zero/explicit-null times become null; absent times remain absent.

## Validation and deliberate differences

Arguments are checked before any request, including registry requests. History size is 1–100; type is exact `all`, `pending`, or `confirmed`; timestamps are digit-only milliseconds from January 1, 2020 through now plus 100×365 days, with an ordered effective time range. Chain lists must contain nonempty chain identifiers. Single-transaction queries require a single chain. Cursors are nonempty, at most 1024 bytes, and restricted to the installed token alphabet; malformed confirmed markers are rejected. No trimming, partial integer parsing, or silent identifier repair is performed.

Token-address filters require an EVM 20-byte hexadecimal address or a 32–44-character base58-shaped Solana address, with chain-family checks when a chain is supplied. Mixed/uppercase EVM filters follow the shared checksum helper's validation. Solana shape checks do not fully decode a public key. Arbitrary-precision helpers reject nonpositive/nonfinite multipliers, and reject scales/exponents outside ±10000 to bound untrusted allocation.

Quota results use **decimal strings** for `quotaUsed`, `quotaLeft`, and `dailyLimit` rather than the installed CLI's binary-floating JSON numbers. This deliberate schema difference retains exact subtraction, including fractional values and totals beyond `u64`; missing/invalid values fall back to zero. The optional `date` is preserved, including explicit null. String-valued backend amounts are exact. The integrated Cargo configuration enables `serde_json`'s `arbitrary_precision` feature, so fractional/out-of-range numeric JSON literals also retain precision before these modules receive them.

## Verification

Unit mapping tests and loopback TCP tests exercise real `Client.request` transport with synthetic envelopes, never live wallet credentials or backend wallet endpoints. Tests cover raw bypass, RWA kinds and share/price rounding, large integer amounts, exact filters/thresholds, validation-before-request, default dates and plural chain queries, pending/confirmed/all cursor states, empty/partial pages, single-transaction normalization, nested optional fields, malformed page responses, and exact quota subtraction.

RED/GREEN execution records are at `<scratch>/portfolio-tdd.log`. The isolated harness is `<scratch>/portfolio-harness`, with its own Cargo package name and **its own `target` directory**. No parent-owned source, manifest, interface, or target directory was modified by the portfolio implementation. The integrated v0.2 CLI exports these commands; isolated harness logs are historical child evidence, not the final integrated gate. See INTEGRATION-REPORT.md for final checks. There is no claim of v0.2 live API compatibility testing.
