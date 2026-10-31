#!/usr/bin/env python3
"""单钱包持久 JSONL 调用示例。--execute 必须显式传入；绝不自动重试。"""
import argparse
import json
import selectors
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="baw-rs")
    parser.add_argument("--wallet-dir", required=True)
    parser.add_argument("--instance-id-file", required=True)
    parser.add_argument("--execute", action="store_true", help="明确允许将输入订单交给真实 fast serve")
    parser.add_argument("--timeout-seconds", type=float, default=300)
    args = parser.parse_args()
    if not args.execute or args.timeout_seconds <= 0:
        parser.error("需要 --execute 且 timeout-seconds > 0；本脚本不是 dry-run")
    command = [args.binary, "fast", "serve", "--wallet-dir", args.wallet_dir,
               "--instance-id-file", args.instance_id_file]
    # One resident process for the entire input stream, not one subprocess per buy.
    failed = False
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          text=True, bufsize=1) as child:
        assert child.stdin is not None and child.stdout is not None
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(child.stdout, selectors.EVENT_READ)
                for line in sys.stdin:
                    if not line.strip():
                        continue
                    # Validate JSON locally but never invent/alter consent or operation IDs.
                    json.loads(line)
                    child.stdin.write(line if line.endswith("\n") else line + "\n")
                    child.stdin.flush()
                    if not selector.select(args.timeout_seconds):
                        raise TimeoutError("response unknown")
                    response = child.stdout.readline(4 * 1024 * 1024 + 65536)
                    if not response.endswith("\n"):
                        raise RuntimeError("missing or oversized response")
                    row = json.loads(response)
                    print(response, end="", flush=True)
                    if row.get("success") is not True:
                        failed = True
                        break
        except (OSError, ValueError, RuntimeError, TimeoutError):
            print("调用失败或结果不确定；已停止，禁止自动重放订单。", file=sys.stderr)
            failed = True
        finally:
            try:
                child.stdin.close()
            except OSError:
                failed = True
            # Do not kill a possibly submitted order on a caller timeout. Drain shutdown;
            # an operator must reconcile unknown outcomes using the journal/backend.
            # Drain before wait so a late response cannot fill the pipe and deadlock.
            # Late output is visible for reconciliation but never changes failure to success.
            for late in child.stdout:
                print(late, end="", flush=True)
                failed = True
            if child.wait() != 0:
                failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
