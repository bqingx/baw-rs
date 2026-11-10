# approvals：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`；`pipeline` 属于实例，不带钱包；`wallets` 管理注册表与 daemon，也不带钱包。

## `approvals`

```text
Token approvals: list, detail, revoke

Usage: baw-rs approvals [OPTIONS] <COMMAND>

Commands:
  list
  detail
  revoke
  approve  `approvals approve`: an ERC-20 `approve(spender, amount)` submitted through the same preview-then-execute path as `contract-call fire`
  help     Print this message or the help of the given subcommand(s)

Options:
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `approvals list`

```text
Usage: baw-rs approvals list [OPTIONS]

Options:
      --spender <address>
          Filter by spender address
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --filterTypes <types>
          Filter by risk type (high_risk, medium_risk, non_interactive, others)
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json

      --limit <number>
          Number of results to return (default: 20) [default: 20]
      --offset <cursor>
          Pagination cursor from previous query
      --timing

      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `approvals detail`

```text
Usage: baw-rs approvals detail [OPTIONS] --binanceChainId <binanceChainId> --tokenContract <address> --spender <address> --type <type>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --tokenContract <address>
          Token contract address
      --json

      --spender <address>
          Spender contract address
      --timing

      --type <type>
          Approval type (approve | permit2)
      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `approvals revoke`

```text
Usage: baw-rs approvals revoke [OPTIONS] --binanceChainId <binanceChainId> --tokenContract <address> --spender <address> --type <type>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --tokenContract <address>
          Token contract address
      --json

      --spender <address>
          Spender contract address
      --timing

      --type <type>
          Approval type (approve | permit2)
      --confirm

      --operation-id <OPERATION_ID>

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```

## `approvals approve`

```text
`approvals approve`: an ERC-20 `approve(spender, amount)` submitted through the same preview-then-execute path as `contract-call fire`.

Measured 2026-11-02: the OKX swap response carries no signal that an approval is missing (an unapproved token still comes back with an empty `signatureData`), so a task that spends a new token fails at simulation unless the allowance exists. Building the approval here keeps the whole path inside baw-rs instead of shelling out to another tool.

Usage: baw-rs approvals approve [OPTIONS] --binanceChainId <binanceChainId> --tokenContract <address> --spender <address> --amount <amount> --from <address>

Options:
      --binanceChainId <binanceChainId>
          Binance chain ID

      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command

      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry

      --tokenContract <address>
          Token contract address

      --json


      --spender <address>
          Spender contract address (OKX `dexContractAddress`)

      --amount <amount>
          Raw amount in the token's smallest unit, or `max` for uint256::MAX (needs --unlimited)

      --timing


      --confirm


      --unlimited
          Permit uint256::MAX, i.e. an approval that stays valid for the whole balance forever. `--amount max` is refused without this flag so an unlimited grant has to be asked for

      --operation-id <OPERATION_ID>


      --policy <POLICY>
          Optional policy file. Pins token contract, value, selector, calldata digest and gas cap; anything that does not match is refused before the preview is requested

      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>


      --from <address>
          Sender wallet address on the target chain

      --gasLimit <n>
          Optional gas limit for the approve call

      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode

          [env: BAW_RS_SKIP_PRECHECKS=]

  -h, --help
          Print help (see a summary with '-h')
```
