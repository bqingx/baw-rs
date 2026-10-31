# In-process Open Four quote (`openfour quote-buy`)

`baw-rs openfour quote-buy` builds the same transaction the official
`agent-id-exclusive-token-launches.mjs quote-buy` builds, without Node, without a wallet
session and without the local simulation.

```bash
baw-rs openfour quote-buy \
  --token 0x6ff638d7bc89cac3567b99cafeaf13f18c51ffff \
  --wallet 0x3D0cD8Be9aDf33b1D5841b2EbE54105aeAF43179 \
  --amount 0.0005 --slippage-bps 2000 --json
```

Output carries `tx.to` / `tx.value` / `tx.inputData` ready for `contract-call preview`, plus
the estimate, the slippage floor and a ready-made `previewCommand`.

## What it reads, in order

1. `Registry.registryCore()` / `registryTools()` — Core and Tools are never hardcoded, so a
   governance upgrade is followed.
2. `Core.tokens(token)` — presetId, quote asset, vault, `exists`, `paused`.
3. In parallel: `Registry.isPresetActive(presetId)`, `Vault.totalRaised()`,
   `Vault.remainingForSale()`, `Vault.phase()`.
4. `Tools.estimateBuyByBudget(token, trader, maxPay, 0, 0x)` — the size of the fill.

Then: `minTokenOut = tokenAmount * (10000 - slippageBps) / 10000`, and
`Core.buyByBudget(token, maxPay, minTokenOut, 0, 0x)` as the calldata. Paying native BNB is
supported; a token whose quote asset is not WBNB is refused rather than encoded wrongly.

## Why no simulation

The reference script simulates locally, which costs roughly 0.3 s. `contract-call preview`
simulates the exact transaction on the backend and reports `simulationCode` / `351805`
before anything is signed, so the local simulation adds latency without adding a safety
layer.

## Refusals (fail closed)

`preset inactive`, `curve paused`, `phase != Trading`, `presetId` mismatch, non-WBNB quote
asset, and an estimate of zero tokens all stop the quote with a message; nothing is built.

## Measured on this host

| | reference script | this command |
|---|---|---|
| wall clock | 467–1100 ms | **67–74 ms** |
| calldata | — | byte-identical to the reference on the same inputs |

Cross-check held: for the same inputs both produced the same `tokenAmountRaw`,
`minTokenOutRaw`, `userPays`, `to`, `value` and `inputData` (0.000 % difference).

## Risk this carries

It duplicates the reference encoding. If Four.meme changes `Core`/`Tools` selectors or the
Core record layout, this module can go stale while the script keeps working. Run the script
against the same inputs after any Four.meme upgrade and diff the calldata; the unit tests
pin the encoding against a captured reference payload, so a silent drift in our encoder is
caught, but a contract-side change is not.
