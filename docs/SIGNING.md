# Preview-bound external signing (v0.2)

Implemented commands: `contract-call preview|execute`; `sign-message preview|execute|result|history`. All calls use the fixed remote wallet API. There is no local wallet key signer, arbitrary endpoint, hidden subprocess, automatic execute retry or daemon.

## Review flow

1. Select the exact wallet directory and original encryption instance.
2. Preview checks current developer mode enabled/unexpired, fetches the actual wallet sender on the selected chain, and rejects missing/ambiguous/mismatching senders. EVM contract fields validate address/checksum, raw wei value, even hex input data and gas units; Solana requires base64 unsignedTx without EVM-only fields. EIP712 requires the exact original JSON-RPC `eth_signTypedData_v4` wrapper string, matching wallet signer and domain chain. Message bytes are forwarded unchanged.
3. A successful preview preserves the complete backend response, including parsed data, risks and authorityChanges. Failed simulation/backend codes block caching/execution; risk-category codes are not confused with API failures. The response adds `previewSha256`, a digest of the exact cached input/output and wallet/action/expiry bindings.
4. Review all parsed data, recipient/calldata/value, signature domain/spenders and every authority change. Execute requires `--confirm --operation-id UNIQUE_ID --accept-preview-sha256 EXACT_DIGEST --requestId ID`. No blanket batch consent is supported. The digest is local explicit review acceptance, not a remote signature or proof that the operation is safe.
5. Execute durably journals the intent before submission, validates cache/digest/wallet/action/request/expiry, then durably consumes the accepted preview **before** current-wallet network revalidation. Developer mode, sender and typed domain are rechecked. Expiry is checked again immediately before the execute POST. An accepted attempt stays consumed even if revalidation or the backend fails. Execution sends `{requestId}` only; never reconstructs an unreviewed input.

Cache expiry is at most 300 seconds from preview receipt, tightened by developer-mode expiry and any provided server bound (`expiresAt`, `expireAt`, `expireTime`, `requestExpireTime`, `expiryTime`). Server expiry fields are not required by the installed wire protocol; absent server bounds still receive the local limit, and malformed present bounds fail closed. The cache binds action, request ID, client identity hash, chain, actual sender and selected directory device/inode. Typed-data domain chain and signer must additionally match actual wallet state. Copying caches to another directory or editing their contents invalidates execution. No supported gas-price cap exists; gasLimit limits units, not fee price or cumulative gas spending.

## Private state and recovery

`signing-previews/` is mode 0700; preview JSON and exclusive consumed markers are 0600. Relative descriptor-based operations reject symlinks, foreign/insecure ownership, hard-linked regular files and oversized/malformed cache data. Backend request IDs are hashed for filenames, not used as paths. Preview publication is atomic and no-clobber; duplicate request IDs cannot replace an earlier review. A permanent exclusive consumed marker blocks replay, including after failed execute or a different operation ID.

The mutation journal persists only intent hash/action/state/timestamps and allowlisted response recovery identifiers; it never persists signature output or raw typed input. The preview cache itself contains the reviewed raw input/output and must be treated as private data. `sign-message result --order-id ID` intentionally returns the requested signature; do not copy that output into debug logs or public reports. Result/history are read-only and retain backend fields, with encoded query parameters and original option names. An order ID is not proof of completed signing; reconcile its result separately.

Offline fixtures cover exact contract/message wire bodies, full risk/authority preservation, cache mode/path rules, digest binding, malformed cache rejection, one-shot consumption after errors, expiry during revalidation, local TTL without a backend expiry, developer mode/sender checks, typed wrapper domain and signer checks, raw result/history behavior and failed inner API codes. No production signing, app confirmation, signature verification against a real wallet, transaction broadcast or chain receipt was performed during this migration.

## Unattended execution (`contract-call run`)

`contract-call run` combines preview and execute in one process. It replaces the human
`--accept-preview-sha256` review with a pinned policy file; `--confirm` and a unique
`--operation-id` are still required.

Policy fields: `version` (1), `chainId`, `to`, `valueWei`, optional `selector`,
optional `calldataSha256`, optional `maxGasLimit`, `allowRisks` (array of risk codes),
`allowAuthorityChanges` (bool), optional `maxTotalWei` (zero-value only for now).

Order of checks: policy parse → chain/target/value/selector/calldata/gas match (no HTTP yet)
→ developer-mode and wallet-address verification → preview → risk and authority-change gate
→ execute with the preview's own digest. Any mismatch aborts; the preview stays consumed on
an ambiguous execute and is never retried.

## Single-call execution (`contract-call fire`)

`contract-call fire` is `run` with the policy gate removed: preview and execute happen in
one process, and there is **no human review of the digest**. `--confirm` and a unique
`--operation-id` are still mandatory, still checked before any wallet access, and still
journaled; the preview's `risks`, `authorityChanges` and `allowanceChanges` are returned in
the output so a caller can log or alert on them after the fact.

Use it when an external script owns the decision (for example a monitor that fires the
moment a contract condition flips) and the extra process start plus the digest handshake
would cost more than the risk it covers. When a human is in the loop, use `preview` +
`execute`; when a fixed intent must be enforced, use `run` with a policy.

Timing note measured on this host: `fire` removes one process start (~0.15 s for the
JavaScript reference, <0.01 s here) and the digest transfer, but the two backend round
trips (preview, execute) are unavoidable.

## Pre-write checks and the resident cache

Every guarded write re-reads `settings/query` (developer mode) and the wallet's chain
addresses before it will preview — two round trips. Inside a resident server those two
reads are reused for `BAW_RS_CHECK_TTL_MS` (default 3000 in `serve`, **0 in a one-shot
command**), which is where a preview followed by its execute stops paying for the same two
reads twice. The cache is dropped whenever the session changes (`set_session`,
`begin_signin`, `invalidate`), is never shared between wallets, and never spans processes.
Set `BAW_RS_CHECK_TTL_MS=0` in the server environment to restore strict per-command reads.

## Skipping the pre-flight reads (`--skip-prechecks`)

Two reads run before any guarded write will preview: `settings/query` (developer mode) and
the wallet's chain addresses. Neither is required by the protocol — the backend rejects a
write when developer mode is off (`351801` / `351802`) and binds the request to the session
— so both are local conveniences: an earlier, clearer error, and the only guard that exists
against signing with a wallet other than the one named in `--from`.

`--skip-prechecks` (or `BAW_RS_SKIP_PRECHECKS=1`, or `--skip-prechecks` on the resident
server to apply it to every request) removes both. When it is on:

- `--from` becomes mandatory; an empty one fails before any network call
- the sender is still format-validated locally, and the preview is bounded by the ordinary
  five-minute window instead of the developer-mode expiry
- a wrong `--from` is no longer caught locally: the request reaches the backend
- measured cost of the two reads on this host: 0.51-0.54 s + 0.43-0.64 s

It is off by default, on every command, and it is the only switch in this tool that removes
a local safety check rather than adding one.
