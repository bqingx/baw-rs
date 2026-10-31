# vaults：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `vaults`

```text
Native named wallets; no implicit wallet or fallback

Usage: vaults --registry <REGISTRY> <COMMAND>

Commands:
  register  
  list      
  status    
  serve     
  stop      
  run       
  help      Print this message or the help of the given subcommand(s)

Options:
      --registry <REGISTRY>  
  -h, --help                 Print help
```

## `vaults register`

```text
Usage: vaults --registry <REGISTRY> register --wallet-dir <WALLET_DIR> --instance-id-file <INSTANCE_ID_FILE> <NAME>

Arguments:
  <NAME>  

Options:
      --wallet-dir <WALLET_DIR>              
      --instance-id-file <INSTANCE_ID_FILE>  
  -h, --help                                 Print help
```

## `vaults list`

```text
Usage: vaults --registry <REGISTRY> list

Options:
  -h, --help  Print help
```

## `vaults status`

```text
Usage: vaults --registry <REGISTRY> status

Options:
  -h, --help  Print help
```

## `vaults serve`

```text
Usage: vaults --registry <REGISTRY> serve

Options:
  -h, --help  Print help
```

## `vaults stop`

```text
Usage: vaults --registry <REGISTRY> stop

Options:
  -h, --help  Print help
```

## `vaults run`

```text
Usage: vaults --registry <REGISTRY> run <NAME> -- <ARGV>...

Arguments:
  <NAME>     
  <ARGV>...  

Options:
  -h, --help  Print help
```
