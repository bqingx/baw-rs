# 地址与单位

- 先区分chainId、token合约地址、原生币占位地址与钱包地址；不能因为ticker相同就跨链复用地址。
- 原生BNB在某些币安报价API用占位地址，不是WBNB合约。Core内盘支付的value是原生BNB的wei。
- contract-call value / fast budget：原生最小单位整数；EVM常用18位，但非原生token按其decimals。
- fast minAmountOut：代币最小单位整数；不是数量小数、百分比、美元价格。无报价就没有相对滑点基准。
- openfour quote-buy amount：人类可读BNB小数字符串；不要将wei传进去造成巨大预算。
- market-order fromTokenQty / wallet send amount：按具体帮助与资产约定的人类可读数量，不能自动从value格式推断。
- 使用整数/Decimal/BigUint，不转JS Number或Python float；显示时四舍五入只影响展示，不修改交易payload。
- 地址必须来自用户确认的输入或可信查询，并核对完整地址及链。合成示例地址不能拿去交易。
- 本skill不维护永久“热门币安全列表”。需要查币时用已授权只读来源，审计结果只陈述事实，不做收益判断。
