# openfour：实际 CLI 参数

由 `scripts/build_skill_reference.py` 从 `baw-rs 0.3.0` 的 `--help` 生成。
这不是运行授权；命令含义、资金确认及对账流程以主 skill 和工作流参考为准。
普通业务命令须带已确认的钱包选择器；`vaults/openfour/fast` 各有独立参数入口。

## `openfour`

```text
Open Four bonding-curve reads (no session needed)

Usage: openfour [OPTIONS] <COMMAND>

Commands:
  quote-buy   Read the curve and build a `buyByBudget` transaction (no session, no signing)
  encode-buy  Offline BNB/WBNB budget encoding, options=0; no quote or RPC
  help        Print this message or the help of the given subcommand(s)

Options:
      --json  Accepted for symmetry with the rest of the CLI; this command always prints JSON
  -h, --help  Print help
```

## `openfour quote-buy`

```text
Read the curve and build a `buyByBudget` transaction (no session, no signing)

Usage: openfour quote-buy [OPTIONS] --token <TOKEN> --wallet <WALLET> --amount <AMOUNT>

Options:
      --json                         Accepted for symmetry with the rest of the CLI; this command always prints JSON
      --token <TOKEN>                Token contract address on BSC
      --wallet <WALLET>              Wallet address that will send the transaction
      --amount <AMOUNT>              Budget in BNB, for example 0.0001
      --slippage-bps <SLIPPAGE_BPS>  Slippage tolerance in basis points (2000 = 20%) [default: 2000]
      --rpc <RPC>                    BSC JSON-RPC endpoint; defaults to the reference endpoint list
  -h, --help                         Print help
```

## `openfour encode-buy`

```text
Offline BNB/WBNB budget encoding, options=0; no quote or RPC

Usage: openfour encode-buy [OPTIONS] --core <CORE> --token <TOKEN> --wallet <FROM> --amount-wei <BUDGET> --min-out-raw <MIN_AMOUNT_OUT>

Options:
      --core <CORE>                   
      --json                          Accepted for symmetry with the rest of the CLI; this command always prints JSON
      --token <TOKEN>                 
      --wallet <FROM>                 
      --amount-wei <BUDGET>           
      --min-out-raw <MIN_AMOUNT_OUT>  
      --allow-zero-min-out            
  -h, --help                          Print help
```
