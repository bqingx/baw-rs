# Implementing a parity module

Every family the JavaScript `baw` CLI exposes gets one module (`src/<family>.rs`) that
owns its `clap` enum and its wire calls. `cli.rs` and `dispatch.rs` are already wired —
**do not edit them**; if a variant is missing there, say so in your report instead.

## Contract of a family module

```rust
pub enum XCommand { ... }                       // clap::Subcommand, nested via subcommands
pub fn is_mutation(c: &XCommand) -> bool        // exactly which leaves write
pub fn mutation_payload(c: &XCommand) -> Result<Value, String>  // intent digest input
pub async fn run(client: &mut Client, c: &XCommand) -> Result<Value, String>
```

`is_mutation` and `mutation_payload` are what make `--confirm --operation-id` mandatory
and what lands in the durable journal. A read-only leaf must never be reported as a
mutation, and a writing leaf must never be omitted — that is the one security-critical
part of this work.

## Wire calls

The client is the only way out:

```rust
let data = client
    .request(Method::GET, &format!("{API}/approve-list"), None)
    .await?;
```

- `API` = `/bapi/defi/v1/public/wallet-direct/agent-wallet`; the origin is fixed and not
  overridable. For non-`agent-wallet` prefixes use the full path.
- `request` returns the already-unwrapped `data` object; it handles cookie rotation,
  401/403 invalidation and API error envelopes.
- No `reqwest` outside `client.rs`, no new dependencies, no `unwrap`/`panic` on network
  or JSON input, no `todo!()`.

## Ground truth

1. `docs/JS-CLI-PARITY.md` — the installed CLI's own `--help` text for every missing
   leaf. Option names, defaults and value shapes come from there, verbatim.
2. `npm root -g`/`@binance/agentic-wallet/dist/index.js` — the wire protocol. Every
   endpoint you call must be one you can point at in that bundle; quote the matched
   string in a comment above the call. **Never invent a path or a field name.**
3. `docs/AUTH.md`, `docs/TRADE.md`, `docs/SIGNING.md`, `docs/PORTFOLIO.md`,
   `docs/SESSION-LIFECYCLE.md` — existing conventions for guards, redaction and output.

If the bundle does not reveal a payload shape, implement what is visible and report the
gap instead of guessing.

## Output and safety conventions

- Return backend data losslessly unless the JS CLI deliberately reshapes it; then match
  the JS shape and say so in a comment.
- Redact nothing that the backend already returned as an address, but never echo
  credentials, cookies or raw error message text from the backend — errors are static
  sanitized strings.
- Show full contract addresses plus symbol when the response carries both.
- Mutations must pass `--confirm` and a unique `--operation-id`; that is enforced in
  `dispatch.rs` through your `is_mutation`, so keep it accurate.

## Tests

Unit tests live in the same file (`#[cfg(test)] mod tests`), using the loopback pattern
from `src/auth.rs` / `src/client.rs`: bind `127.0.0.1:0`, serve canned HTTP replies, build
`Client::test_client(credentials, origin)` (available under `cfg(test)`), assert on both
the returned JSON and the captured request path/body.

Minimum per module:

- one test per leaf that asserts the request path, method and body fields;
- a mutation-flag test: every writing leaf is `is_mutation == true` and every read leaf is
  `false`;
- one test for a backend error envelope (assert the message is the sanitized one);
- one test for a leaf that must not reach the network when required args are invalid.

## Gates before you report

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd <repo>
CARGO_TARGET_DIR=<scratch>/baw-rs-target cargo fmt
CARGO_TARGET_DIR=<scratch>/baw-rs-target cargo test --lib <your_module>::
CARGO_TARGET_DIR=<scratch>/baw-rs-target cargo clippy --all-targets -- -D warnings
```

`cargo test --lib` is filtered to your module while iterating; the full suite takes ~2.5
minutes. `clippy -D warnings` must be clean for your file. Do not touch `verification/`
or run `scripts/verify.sh` — the parent owns release evidence.
