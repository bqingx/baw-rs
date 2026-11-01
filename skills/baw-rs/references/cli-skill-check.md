# skill-check：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带 `--wallet <NAME>`（或传统 `--wallet-dir` 组合）；`wallets/openfour` 各有独立参数入口。

## `skill-check`

```text
Check whether a skill has a newer version available

Usage: baw-rs skill-check [OPTIONS] --skill-name <SKILL_NAME> --current-version <CURRENT_VERSION>

Options:
      --skill-name <SKILL_NAME>
          Skill name to check
      --wallet <NAME>
          Name of a registered wallet. Required for every wallet-scoped command unless the legacy --wallet-dir pair is given
      --current-version <CURRENT_VERSION>
          Current version to compare against
      --registry <PATH>
          Registry directory. Defaults to $BAW_RS_REGISTRY, else $HOME/wallets/registry
      --json
          
      --timing
          
      --confirm
          
      --operation-id <OPERATION_ID>
          
      --accept-preview-sha256 <ACCEPT_PREVIEW_SHA256>
          
      --warm
          Route this command through the resident warm server when it is running
      --skip-prechecks
          Skip the two pre-write reads (developer-mode settings, wallet addresses) and trust `--from` as the sender. Removes the local guard against sending from a wallet other than the one named; the backend still enforces developer mode [env: BAW_RS_SKIP_PRECHECKS=]
  -h, --help
          Print help
```
