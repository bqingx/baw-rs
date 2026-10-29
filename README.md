# baw-rs 0.3.0

Native Rust BAW CLI, scoped to the installed `@binance/agentic-wallet` 1.12.0 wire protocol. This release completes the requested v0.1 omissions; it is **not the entire JavaScript CLI**. It neither invokes nor replaces the installed `baw`. Linux is the tested platform.

No prebuilt binary is shipped in this repository. Build it yourself; only a build you
produced has a hash you can check. `scripts/verify.sh` reproduces every gate artefact
in `verification/` (fmt, clippy, test, release build, source manifest, leaf commands).

## Build and select a wallet

```sh
cargo build --release --target-dir /root/.hermes/cache/scratch/baw-integration-target
/root/.hermes/cache/scratch/baw-integration-target/release/baw-rs \
  --wallet-dir /absolute/wallet-directory --instance-id EXACT-ENCRYPTION-IDENTITY --json wallet status
```

Both selectors are required. Put them before the command, or use `BINANCE_BAW_DIR` and `BINANCE_INSTANCE_ID`. There is no implicit default wallet, MAC-derived encryption identity, global keyring access, or fallback to another session. The instance must match the identity that encrypted the selected file. Existing encrypted sessions and legacy plaintext client UUIDs load without migration writes. Only explicit signin may create a new identity in an absent selected directory.

### `--instance-id` is a credential, not a machine identity

The AES-256-GCM key for `session.json` is derived from that string alone:

```text
PBKDF2-HMAC-SHA256(password = instance, salt = "baw-agentic-wallet-v1:" + instance, 100k iters)
```

There is no keyring, no hardware root, no per-host secret. Consequences, all fail-closed:

- **Lost instance string → the session is unrecoverable.** Nothing can re-derive the key.
- **Instance string + `session.json` together → the login cookie is recoverable by anyone.**
  Treat the instance string like a password: keep it in a 0600 file or an environment
  variable, not in shell history or a command line that lands in `ps`.
- Selecting a **different** instance against an existing directory fails with
  `invalid encrypted credential` and leaves the file byte-identical — no repair, no
  migration, no rewrite (covered by `fails_closed_on_wrong_instance_without_mutation`).


## Completed command inventory

`verification/leaf-commands.txt` is the authoritative list, generated from the built
binary by `scripts/leaf_commands.py`. Every leaf of the installed `baw` 1.12.0 is
implemented except `tracker ws` (WebSocket transport). `docs/JS-CLI-PARITY.md` holds the
reference `--help` for each one; family modules:

| family | module | leaves |
|---|---|---|
| auth / wallet / market-order / contract-call / sign-message | `auth`, `client`, `trade`, `signing` | the original 24 |
| approvals | `approvals` | list, detail, revoke |
| limit-order | `limit_order` | buy, sell, list, cancel |
| prediction | `prediction` | 15 leaves (category / market / position / order / trade) |
| defi | `defi` | 11 leaves (protocol / investment / position / deposit / redeem / lp / claim / preview) |
| x402-payment | `x402` | preview, sign |
| wallet ops | `wallet_ops` | speed-up, cancel, resolve-domain |
| tracker | `tracker` | 15 leaves (token / tx / follow / group / address) — `ws` parses, returns a clear unimplemented error |
| signal | `signal` | 15 leaves (list / strategy / backtest / explore / credits / wallet-group) |
| leaderboard | `leaderboard` | 7 leaves (query / analyze / alpha-radar / presets) |
| self-checks | `selfcheck` | cli-check, skill-check |

The original surface, unchanged:

```text
auth signin
auth verify --qrCodeId QR_ID
auth signout
wallet status
wallet address
wallet chains
wallet settings
wallet gas-price --binanceChainId CHAIN
wallet tx-lock --binanceChainId CHAIN
wallet agent-id
wallet balance [--raw] [--symbol SYMBOL] [--tokenAddress ADDRESS]
               [--binanceChainId CHAIN] [--hideLowValue true|false]
wallet tx-history [--type all|pending|confirmed] [--binanceChainId CHAIN]
                  [--size 20] [--nextCursor CURSOR] [--tx HASH]
                  [--startTime MILLISECONDS] [--endTime MILLISECONDS]
wallet left-quota
wallet send --binanceChainId CHAIN --tokenAddress TOKEN --recipient ADDRESS
            (--amount DECIMAL | --max) [--gasLevel LOW|MEDIUM|HIGH]
market-order quote --binanceChainId CHAIN --fromToken ADDRESS --toToken ADDRESS
                   --fromTokenQty DECIMAL [--slippage auto|PERCENT]
market-order swap --binanceChainId CHAIN --fromToken ADDRESS --toToken ADDRESS
                  --fromTokenQty DECIMAL [--slippage auto|PERCENT]
                  [--gasLevel LOW|MEDIUM|HIGH] [--mev true|false] [--acknowledge-token-risk]
market-order list [--orderId ID] [--binanceChainId CHAIN] [--status PENDING|FINISHED|FAILED]
                  [--fromToken ADDRESS] [--toToken ADDRESS] [--page 1] [--pageSize 20]
                  [--startTime MILLISECONDS] [--endTime MILLISECONDS]
contract-call preview --binanceChainId CHAIN --from ADDRESS
                      [--to ADDRESS] [--value WEI] [--inputData HEX] [--gasLimit UNITS]
                      [--unsignedTx SOLANA_BASE64]
contract-call execute --requestId PREVIEW_ID
contract-call run --binanceChainId CHAIN --from ADDRESS --to ADDRESS
                  --policy /path/to/policy.json
                  [--value WEI] [--inputData HEX] [--gasLimit UNITS]
sign-message preview --binanceChainId CHAIN --signType EIP712 --message JSON_RPC_STRING
sign-message execute --requestId PREVIEW_ID
sign-message result --order-id ORDER_ID
sign-message history [--binanceChainId CHAIN] [--limit 20] [--nextToken CURSOR]
                     [--startTime MILLISECONDS] [--endTime MILLISECONDS] [--sortType ASC|DESC]
batch
```

Run each command's `--help` for its exact grammar. EVM contract previews require `to`; Solana requires `unsignedTx` and rejects EVM-only fields. EIP712 messages retain the original `eth_signTypedData_v4` JSON-RPC wrapper string and must bind its signer and domain chain to the selected wallet.

## Resident warm mode

`serve` keeps one decrypted session, one connection pool and a 60-second
developer-mode/wallet-address probe cache inside a single process, listening on
`<wallet-dir>/.baw-rs.sock` (directory 0700, socket 0600, peer UID checked).
Prefix any command with `--warm` to route it through the server; if no server is
running the call silently falls back to a normal cold execution.

```sh
./baw-rs --wallet-dir /abs/wallet --instance-id ID --json serve &
./baw-rs --wallet-dir /abs/wallet --instance-id ID --warm --json wallet status
```

Measured on this host (BSC, same wallet): `wallet status` 104 ms cold → 10–20 ms
warm; `contract-call preview` 500 ms cold → 250 ms warm; `contract-call run`
1.2 s cold → 0.9 s warm. The remaining cost is server-side: the execute request
alone is roughly 600 ms, so a preview+execute pair cannot go below that even with
zero local overhead. `serve` and `batch` are rejected over the socket.

## Unattended execution with a pinned policy

`contract-call run` performs preview and execute in a single process with no human digest review. It is authorized by a JSON policy file instead of `--accept-preview-sha256`, and it still requires `--confirm` and a unique `--operation-id`. A mismatched policy is refused **before** any preview request is sent.

```json
{
  "version": 1,
  "chainId": "56",
  "to": "0xContractAddress",
  "valueWei": "0",
  "selector": "0x1234abcd",
  "calldataSha256": "64 lowercase hex of the full 0x calldata (optional)",
  "maxGasLimit": 300000,
  "allowRisks": [],
  "allowAuthorityChanges": false
}
```

Checks, in order: chain ID, target address (case-insensitive), exact value in wei, 4-byte selector, optional full-calldata SHA-256, gas-limit ceiling, then the returned preview's risk codes, `authorityChanges` and `allowanceChanges`. Anything not explicitly allowed aborts the run. There is no retry: an ambiguous execute is reported, never repeated. EVM only. A cumulative spend cap (`maxTotalWei`) exists but currently only accepts zero-value calls.

## Mutation authority and recovery

**All auth commands, swaps, sends, and signing executes require both `--confirm` and a fresh `--operation-id ID`.** These global flags may appear before or after the command. IDs contain 1–64 ASCII letters, digits, `-` or `_`. Missing consent fails before session access or HTTP. A new ID is a new authorization, not permission to retry an uncertain operation.

A private durable intent record is synced under the selected directory's `.baw-rs-operations/` before any mutation submit. The record stores an intent hash, action, state, timestamps and allowlisted successful-response recovery identifiers, not credentials, raw typed messages or signatures. Existing IDs are permanently refused, including after errors/crashes. A private per-wallet lock prevents concurrent guarded mutations. Do not delete journals to retry: first reconcile order/transaction/signature history. `RESPONSE_RECEIVED` means an API response was obtained, **not that a trade filled or transaction confirmed**; inspect status. `FAILED_OR_UNKNOWN`/`SUBMITTING` require reconciliation. A failed inner order code is an error, never success. Uncertain swap/send transport failures return `UNCERTAIN`, `retrySafe:false`; no automatic resubmission occurs.

Signing execute additionally requires `--accept-preview-sha256 EXACT_DIGEST` from its successful preview output. Review the whole output, including `parsedTx`/`parsedMessage`, `risks` and **all `authorityChanges`**, before accepting. The private preview cache binds the exact response/input to request, action, client identity, selected wallet directory inode/device, actual sender and chain. Expiry is at most five minutes and further limited by developer-mode expiry and any server expiry. Execute rechecks current developer mode and wallet, rechecks time immediately before POST, and consumes the accepted preview **before network revalidation**, even when that attempt errors. Replaying with another operation ID cannot reuse it. Obtain/review a new preview only after reconciling an uncertain prior submission.

Auth signin returns QR URL/ID/pairing code; it never opens a browser. Verify polls at two-second intervals within the original QR expiry and a persisted five-minute cap. Session commits occur only after CONNECTED + CREATED, with encrypted atomic replacement and stale-write protection. Signout clears the local session while preserving client identity even if remote logout fails, explicitly reporting remote uncertainty. See [auth](docs/AUTH.md) and [signing](docs/SIGNING.md).

## Read-only batch remains compatible

```sh
printf '%s\n' '["wallet","status"]' '["wallet","balance"]' \
  '["wallet","balance","--raw"]' '["wallet","tx-history","--type","pending"]' |
  ./baw-rs --wallet-dir /absolute/wallet-directory --instance-id EXACT-ENCRYPTION-IDENTITY batch
```

Batch accepts read-only **wallet** commands only, one JSON argv array per line, retaining one session/client/connection pool. All mutation commands and per-line global overrides are rejected. Top-level `batch --confirm`, operation IDs, and preview acceptance flags are also rejected; blanket mutation authority is impossible. Errors are per-line; processing continues. Input is bounded to 65,536 bytes/line with oversized-line recovery, strict UTF-8/string-array validation and final unterminated-line handling. Exit 0 means all inputs succeeded; broken stdout pipes exit quietly. `--timing` prints only `command_ms=...` to stderr, never arguments or bodies.

## Output and deliberate compatibility differences

JSON is always emitted (`--json` is accepted). Each command produces `{"success":true,"data":...}` or `{"success":false,"error":{"message":"sanitized error"}}`.

- Default balance now normalizes RWA shares, prices, exact value, EVM checksum and filters; `--raw` retains the unmodified backend tokenList and skips registry loading/conversion/filtering. Raw amounts are **not** necessarily RWA shares. Normalized balances and quota amounts are decimal strings, not JS floating numbers. JSON numeric backend decimals also retain arbitrary precision. See [portfolio](docs/PORTFOLIO.md).
- Settings deliberately remain the v0.1 **lossless composite** `{settings,tradingLimit,login}`, not the original renamed/coerced display object. Gas price, tx-lock and Agent ID return backend data. Ordinary query cookie rotation is in memory only, adopted only after HTTP/API success; only verified auth commits persist it.
- Quotes retain quoteId, feeDetail, gasDetails and backend metadata. Signing previews retain authorityChanges the original CLI discards. Signature result output intentionally returns the requested signature; it is never added to logs/journals.
- RWA registry is fetched fresh per normalized command; no original global/stale disk cache. Failures propagate. Solana address identity is case-sensitive. Strict input checks reject malformed amounts, addresses and ambiguous combinations rather than trimming or guessing.
- Swaps use settings/quota/tx-lock checks and a fresh quote; this is **not quote-ID-bound execution**. Non-native targets require `--acknowledge-token-risk`, explicitly not a token audit or backend-policy override. Default slippage is `auto`. Unsupported MEV selections fail rather than silently dropping protection. Ondo requires `--mev false` and default MEDIUM gas selection (no promised endpoint fee selection). See [trade](docs/TRADE.md).
- Trade acceptance is SUBMITTED, not FILLED. App confirmation stays PENDING_CONFIRMATION. History/status and chain confirmation must be checked separately. Backend admission remains authoritative; local quota checks do not prove the requested trade fits a USD cap. `gasLimit` is units, **not gas-price or cumulative gas-cost protection**; the protocol offers no fee-price-cap fields here.
- Fixed release origin is `https://www.binance.com`; no URL override, redirects, proxies, subprocess delegation or automatic write retries. Requests have 10-second connect/30-second total timeout and 4-MiB response bounds. Only test builds can inject loopback HTTP. Wire version metadata stays 1.12.0; User-Agent is baw-rs/0.2.0. Credential files require owner-only 0700 directory/0600 regular-file modes. Wrong instances fail closed without repair/migration.

## Deliberately outside this migration

`tracker ws` (WebSocket push subscriptions), arbitrary endpoints, browser/UI/keyring
integration, human terminal QR rendering, automatic order polling, a trading daemon and
fee-price caps are **not** included. `tracker ws` parses as a leaf and returns
`tracker ws requires websocket support, not implemented` rather than silently doing
nothing; making it work needs a WebSocket client dependency. No unsupported path
silently falls back to the original executable.

## Verification boundaries

The v0.2 verification is offline: real Rust execution with synthetic private session files and loopback HTTP fixtures. No live wallet credentials were read, no authenticated production request or real signing/trade/logout was performed, and no installed CLI/service/configuration was changed. See [INTEGRATION-REPORT.md](INTEGRATION-REPORT.md) for exact final test/build output and executable path. `LIVE-VERIFICATION.json` and `BENCHMARK.json` are **historical v0.1** read-only observations, not v0.2 live validation or trade/sign latency evidence.

```sh
cargo fmt --check
cargo clippy --all-targets --target-dir /root/.hermes/cache/scratch/baw-integration-target -- -D warnings
cargo test --target-dir /root/.hermes/cache/scratch/baw-integration-target
cargo build --release --target-dir /root/.hermes/cache/scratch/baw-integration-target
```
