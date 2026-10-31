# fast：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `fast`

```text
Explicit opt-in native-budget OpenFour path

Usage: fast <COMMAND>

Commands:
  build  Offline only: one Buy JSON object on stdin, no session or RPC
  serve  Persistent stdin JSONL, one pinned wallet; no per-order CLI subprocess
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## `fast build`

```text
Offline only: one Buy JSON object on stdin, no session or RPC

Usage: fast build

Options:
  -h, --help  Print help
```

## `fast serve`

```text
Persistent stdin JSONL, one pinned wallet; no per-order CLI subprocess

Usage: fast serve --wallet-dir <WALLET_DIR> --instance-id-file <INSTANCE_ID_FILE>

Options:
      --wallet-dir <WALLET_DIR>              
      --instance-id-file <INSTANCE_ID_FILE>  
  -h, --help                                 Print help
```
