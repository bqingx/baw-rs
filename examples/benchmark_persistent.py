#!/usr/bin/env python3
"""Run ONLY through the ignored Rust loopback fixture; never a live registry.

BAW_RS_CHECK_TTL_MS=3000 BAW_RS_BENCH_BINARY=$CARGO_TARGET_DIR/release/baw-rs \
 cargo test --release persistent_executor_loopback_benchmark -- --ignored --nocapture
Both paths submit identical synthetic generic calls to the same fixture backend.
The old path includes real CLI process startup; typed prepare is reported separately.
Durable operation/preview fsyncs remain enabled. No real-wallet credentials or RPC.
"""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import time
import sys
sys.dont_write_bytecode = True
from persistent_client import PersistentClient


def quantiles(values):
    rows = sorted(values)
    return {f"p{p}": rows[max(0, math.ceil(len(rows) * p / 100) - 1)] for p in (50, 95, 99)}


def checked(row):
    if row.get("success") is not True:
        raise RuntimeError(f"synthetic benchmark request failed: {row}")
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--registry", required=True)
    parser.add_argument("--samples", type=int, default=100)
    args = parser.parse_args()
    marker = json.loads((Path(args.registry) / "loopback-benchmark.json").read_text())
    if not os.environ.get("BAW_RS_LOOPBACK_FIXTURE") or marker.get("token") != os.environ["BAW_RS_LOOPBACK_FIXTURE"] or not marker.get("origin", "").startswith("http://127.0.0.1:"):
        raise RuntimeError("refusing non-fixture registry; invoke through the Rust loopback test")
    if not 1 <= args.samples <= 120:
        parser.error("samples must be 1..120 (bounded task history)")
    sender, target = "0x" + "1" * 40, "0x" + "2" * 40
    call = dict(binance_chain_id="56", **{"from": sender}, to=target,
                value="0", input_data="0x12345678", gas_limit=100000)
    rows = {key: [] for key in ("old_run_cli_ms", "typed_prepare_ms", "typed_trigger_ms",
            "typed_prepare_plus_trigger_ms", "typed_first_request_gap_ms", "typed_network_wait_ms",
            "typed_local_handler_ms", "typed_response_to_next_request_ms", "typed_ingress_ms", "typed_queue_ms")}
    with PersistentClient(args.registry + "/daemon.sock") as client:
        checked(client.request(op="ping"))
        # Warm both paths without including cold session decryption in percentiles.
        for i in range(args.samples + 5):
            id_old, id_typed = f"old-{i}", f"typed-{i}"
            cmd = [args.binary, "--registry", args.registry, "--wallet", "alice",
                   "--json", "--confirm", "--operation-id", id_old, "contract-call", "fire",
                   "--binanceChainId", "56", "--from", sender, "--to", target,
                   "--value", "0", "--inputData", call["input_data"], "--gasLimit", "100000"]
            # Alternate order to reduce monotonic warmup / load bias.
            def old():
                start = time.perf_counter_ns()
                p = subprocess.run(cmd, capture_output=True, text=True, check=True)
                elapsed = (time.perf_counter_ns() - start) / 1e6
                checked(json.loads(p.stdout))
                return elapsed
            def typed():
                start = time.perf_counter_ns()
                checked(client.prepare("alice", id_typed, time.time_ns() // 1_000_000 + 60000, call))
                prep = (time.perf_counter_ns() - start) / 1e6
                start = time.perf_counter_ns()
                result = checked(client.trigger("alice", id_typed, confirm=True))
                elapsed = (time.perf_counter_ns() - start) / 1e6
                return prep, elapsed, result
            if i % 2:
                prep, trigger, result = typed()
                legacy = old()
            else:
                legacy = old()
                prep, trigger, result = typed()
            if i < 5:
                continue
            http, metrics = result["data"]["http_metrics"], result["metrics"]
            rows["old_run_cli_ms"].append(legacy)
            rows["typed_prepare_ms"].append(prep)
            rows["typed_trigger_ms"].append(trigger)
            rows["typed_prepare_plus_trigger_ms"].append(prep + trigger)
            rows["typed_first_request_gap_ms"].append(http["first_request_gap_ms"])
            rows["typed_network_wait_ms"].append(http["network_wait_ms"])
            rows["typed_local_handler_ms"].append(metrics["handler_ms"] - http["network_wait_ms"])
            rows["typed_response_to_next_request_ms"].extend(http["response_to_next_request_ms"])
            rows["typed_ingress_ms"].append(metrics["ingress_decode_ms"])
            rows["typed_queue_ms"].append(metrics["queue_ms"])
    print(json.dumps({"samples": args.samples, "warmups": 5, "units": "ms", "percentiles":
                      {key: quantiles(value) if value else None for key, value in rows.items()},
                      "limitations": ["Loopback synthetic backend, not production network or chain latency",
                      "Old includes process startup; typed trigger excludes prepare (also reported combined)",
                      "HTTP await time includes scheduler/transport; not remote server processing",
                      "Local handler includes synchronous durable journal/preview fsync",
                      "No wallet work queue: competing work fails BUSY; queue_ms=0",
                      "Percentiles use nearest rank, no throughput/concurrency claim"]}, indent=2))


if __name__ == "__main__":
    main()
