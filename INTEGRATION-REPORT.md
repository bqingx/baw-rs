# BAW Rust v0.2 integration report

> **Historical document — scope limited to v0.2.0.**
> The gates quoted below ran on `v0.2.0` at `/root/baw-rs`. The source in this
> repository is `0.2.2` and differs in 9 files, so **none of the counts or hashes in
> this report describe the current tree**. They are retained as the record of that
> earlier review pass. The shipped binary hash (`d0e203…`, 5,564,520 bytes) refers to a
> build that was not included in the source drop and cannot be checked from here.
> For the current tree see `verification/` (regenerate with `scripts/verify.sh`) and
> `CHANGELOG.md`.

## Outcome

Requested migration is integrated and offline-verified. Release version: **0.2.0**. All previously interrupted modules are compiled into the real CLI; no original CLI fallback, production stubs, todo!/unimplemented! paths, fake responses or live-wallet execution were introduced.

**Delivered executable:** `/root/baw-rs/dist/baw-rs`

Build executable: `/root/.hermes/cache/scratch/baw-integration-target/release/baw-rs`

Both files have identical SHA-256: `d0e203355a972778d213e19a64f6a7e138717f479cc647f00d95e0b78c3d9537`.

Executable size: **5,564,520 bytes**. Actual release output: `baw-rs 0.2.0`. Project-local copy avoids depending on scratch-target retention. No system executable was replaced.

## Exact final checks

All final commands exited **0**, using `/root/.hermes/cache/scratch/baw-integration-target` for compilation:

| Gate | Actual outcome | Saved output |
|---|---|---|
| `cargo fmt --check` | clean | `verification/fmt.log` |
| `cargo clippy --all-targets --target-dir ... -- -D warnings` | clean, no warnings | `verification/clippy.log` |
| `cargo test --target-dir ...` | **109 passed, 0 failed, 0 ignored** | `verification/cargo-test.log` |
| `cargo build --release --target-dir ...` | optimized release built | `verification/release-build.log` |
| Project-local release smoke | version, all **24** command help paths and unauthorized signin checked | `RELEASE-SMOKE.json` |

Exact test breakdown: **100 library unit/cross-module tests**, **5 existing CLI integration tests**, **3 mutation-consent executable tests**, **1 port-contract parser test**. Main and doctest targets contain 0 tests; they are **not** counted as feature coverage. The port-contract test checks all **17 previously missing command invocations**, including normalized/raw balance variants. Every targeted regression run actually selected and executed one test; no zero-selection filtered test was treated as success.

Final test run reported library completion in 142.67 seconds, existing CLI tests in 2.61 seconds, mutation tests in 0.07 seconds and port-contract in 0.01 seconds. These are offline test durations, not wallet-operation latency claims.

`verification/source-sha256.json` records the checked source/manifests/tests. Saved logs are copied from the actual scratch gate output, not reconstructed test transcripts. No credential-bearing files or live HTTP responses are included.

## Completed command inventory

All 24 executable leaf commands passed a real release `--help` invocation:

- `auth signin`, `auth verify`, `auth signout`.
- `wallet status`, `wallet address`, `wallet chains`, `wallet settings`, `wallet gas-price`, `wallet tx-lock`, `wallet agent-id`, `wallet balance` (normalized and `--raw`, including RWA), `wallet tx-history`, `wallet left-quota`, `wallet send`.
- `market-order quote`, `market-order swap`, `market-order list`.
- `contract-call preview`, `contract-call execute`.
- `sign-message preview`, `sign-message execute`, `sign-message result`, `sign-message history`.
- `batch` (read-only wallet commands; preserves v0.1 session/client reuse and malformed-line handling).

CLI options and deliberate schema differences are documented in README.md and docs/AUTH.md, docs/PORTFOLIO.md, docs/TRADE.md, docs/SIGNING.md. Auth calls require the same explicit mutation consent as sends/swaps/executes. Preview/result/history are not execute authorization.

## Integration and safety work

- Exported auth/auth_store/portfolio/RWA/trade/signing; wired their real clap arguments and dispatch from main. Added `src/dispatch.rs`, the single guarded CLI submit path. Updated Cargo.toml/Cargo.lock to 0.2.0 and enabled arbitrary-precision JSON numeric parsing.
- Kept v0.1 wallet query behavior and connection-reusing batch; intentionally changed balance-without-raw tests because normalized balance now exists. Raw mode remains lossless/backend-unit output; settings remains the documented lossless composite rather than original renamed numeric display.
- Preflight rejects missing confirmation/ID **before wallet/session access or network**. Executable tests check each mutation family with neither flag and either flag alone. Top-level blanket batch consent and unreviewed signing execute also fail before session access. Per-line batch globals/mutations are refused.
- Guarded writes durably create/sync their intent journal before any submit; duplicate IDs remain refused after completion, errors or process lifetime. A cross-module loopback send test inspects SUBMITTING on disk before both registry and transfer requests, verifies exact high-precision amount bytes and successful recovery hash, and proves replay never reaches HTTP. Journals contain intent digests rather than credentials/raw messages/signatures.
- External signing binds accepted exact preview digest to action, request, client, selected wallet directory, chain and actual sender. Preserves all risk and authority data. Current developer mode/wallet/domain are revalidated, and a durable one-shot marker blocks another execute even under a new operation ID. Failure/replay, binding tamper and expiry cases use synthetic HTTP/files.
- Auth persists encrypted pending/committed sessions in private synthetic files using atomic replacement, fsync, directory locking and baseline checks; tests demonstrate changed inode, decryptable original-compatible credentials, no plaintext leaks, stale-commit refusal and separate signin/verify operation lifetimes. Signout preserves identity, clears local even on remote error, and reports uncertainty.
- Decimal helpers preserve string amounts, exact RWA conversion/rounding and large base units. Added regression against JSON numeric rounding: `9007199254740993.123456789123456789` must remain exact, not `9007199254740994.0`.
- Errors and accepted orders are not fills. Inner failed market-order and transfer API codes cannot be reported as successful submission. Backend error-message text is sanitized. HTTP/envelope ambiguity does not trigger automatic write retry.

## Observed RED → GREEN fixes

Existing failures were run first: the initial compiled v0.1 suite passed 34 library + 5 CLI tests, then failed the port-contract test at `auth signin`. Exporting interrupted modules exercised 88 real library tests before CLI integration.

Further failing regressions were observed and fixed individually:

| Regression | Observed RED | Implemented correction / GREEN |
|---|---|---|
| Unauthorized command families | signin reported unsupported command instead of consent requirement | real parser + pre-session mutation preflight; executable tests pass |
| Revalidation failure reused accepted preview | second attempt reached HTTP rather than returning consumed | consume accepted preview before revalidation; selected test passes |
| Preview without backend expiry | rejected `preview expiry bound missing` | mandatory local five-minute TTL, tightened by valid server/dev bounds; selected test passes |
| Expiry during wallet revalidation | continued to execute request after deadline | recheck immediately before execute POST; selected test passes |
| Inner market error message leaked backend text | synthetic secret appeared in error | static sanitized message/bounded code; selected test passes |
| Native logged-out file could not load | encrypted empty session rejected | accept authenticated encrypted logout state, still reject malformed/missing/decryption failures; selected lifecycle test passes |
| Expired-session signin blocked renewal | explicit signin returned session-expired error | start fresh authorized login after failed/disconnected query, clear old cookie, omit Cookie entirely when empty; selected test passes |
| Signed-out signature query attempted HTTP | timeout before local session error | dispatch rejects missing session before query/cache/journal; selected test passes |
| Numeric JSON amount precision | rounded to `9007199254740994.0` | arbitrary_precision parser enabled; selected test passes |
| Inner transfer error reported SUBMITTED | txHash + failed data.code returned successful result | reject failed inner code before status mapping; selected test passes |

Clippy initially caught unnecessary owned BigDecimal comparisons; these were changed to equivalent integer comparisons without disabling the lint. A full test run caught the old v0.1 User-Agent test assertion after the version bump; it was deliberately updated to v0.2.0, while wire compatibility metadata remains 1.12.0. The final full run above passed.

## Files and scope

Primary implementation changes: `src/lib.rs`, `src/cli.rs`, `src/main.rs`, new `src/dispatch.rs`, `src/client.rs`, `src/session.rs`, integrated/fixed `src/auth.rs`, `src/auth_store.rs`, `src/signing.rs`, `src/trade.rs`; formatter/lint cleanup also touched `src/execution.rs`, `src/portfolio.rs`, `src/rwa.rs`. Tests: existing tests/cli.rs intentionally updated for normalized balance, new tests/mutation_consent.rs, existing tests/port_contract.rs now passes, additional module/cross-module regressions. Manifests, README and feature docs updated; new report, release smoke JSON, verification logs/source hashes and `dist/baw-rs` delivered.

The directory is not a Git repository, so no Git diff/commit is claimed. Work stayed within /root/baw-rs and scratch build/log paths. Original `/usr/bin/baw`, `/root/baw-ext`, shell/Hermes/Camofox/service/dependency configurations and wallet directories were not modified.

## Parent verification after integration

The parent independently reran version, formatting, strict all-target Clippy, and the full suite: **109 passed, 0 failed**. The project-local executable was byte-for-byte identical to the release build, SHA-256 `d0e203355a972778d213e19a64f6a7e138717f479cc647f00d95e0b78c3d9537`.

A live **read-only** status check was attempted with the explicitly selected existing w1 session. It returned a successful API response with status **UNCONNECTED**. Verification stopped there; no login/verify flow, signature/preview, swap, transfer or signout was initiated. The session file hash was unchanged. See `LIVE-VERIFICATION-v0.2.json` for the sanitized observation. Accordingly, live v0.2 query/quote parity, performance regression and execution remain unverified until the wallet is connected and those operations are separately authorized.

## Remaining live-verification limits

There is **no remaining offline gate blocker**. This is not a claim of production API/account parity. All automated HTTP tests use synthetic loopback servers, and all test credential/session fixtures are synthetic temporary files. The integration agent made no production calls or live credential reads. The parent's sole live check is documented above and returned UNCONNECTED; no auth session was created, real signature requested, transaction transferred/swapped, or live session signed out. No production listener/daemon was started.

A separately authorized operator must verify actual QR/app login, backend/device/session policy, currently returned settings/preview schemas, real RWA coverage, app confirmation, order admission, signature result correctness and chain receipts. Historical LIVE-VERIFICATION.json/BENCHMARK.json are v0.1 artifacts only. Do not infer swap/sign latency or live success from offline tests/help checks.

Deliberately outside the requested omission list: DeFi, prediction, limit orders, approvals, x402, domain resolution, Agent ID claims, speed-up/cancel, keyring/browser UI, automatic order polling, arbitrary endpoints and daemon operation. GasLimit is units; this backend CLI protocol provides no fee-price cap, and none is promised. Local token-risk acknowledgement is not an audit, quota preflight is not a provable spend bound, and HTTP acceptance is not transaction confirmation. Private preview caches contain reviewed raw input/output; requested signature result output is intentionally sensitive and must not be logged publicly.
