"""Offline independent oracle. Optional regeneration: eth-abi + eth-hash[pycryptodome]."""
import json
from pathlib import Path
from eth_abi import encode
from eth_hash.auto import keccak

cases = []
for budget, minimum in [(1, 1), (2**128, 2**160 + 123), (2**256 - 1, 2**256 - 1)]:
    token = "0x1234567890abcdef1234567890abcdef12345678"
    payload = keccak(b"buyByBudget(address,uint256,uint256,uint256,bytes)")[:4]
    payload += encode(["address", "uint256", "uint256", "uint256", "bytes"],
                      [token, budget, minimum, 0, b""])
    cases.append({"token": token, "budget": str(budget), "minAmountOut": str(minimum),
                  "inputData": "0x" + payload.hex()})
Path(__file__).with_name("openfour_budget.json").write_text(json.dumps(cases, indent=2) + "\n")
