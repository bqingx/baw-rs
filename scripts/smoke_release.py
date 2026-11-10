#!/usr/bin/env python3
"""Smoke an existing CLI in temporary empty wallet directories. No login or chain calls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def smoke(binary):
    checks = []
    with tempfile.TemporaryDirectory(prefix="baw-release-smoke-") as tmp:
        root = Path(tmp)
        home = root / "home"
        home.mkdir(mode=0o700)
        registry = root / "registry"
        env = {k: os.environ[k] for k in ("PATH", "LANG", "TMPDIR") if k in os.environ}
        env["HOME"] = str(home)
        base = [str(binary), "--registry", str(registry)]

        def cli(name, args, success=True):
            out = subprocess.run(base + args, env=env, capture_output=True,
                                 text=True, timeout=10, check=False)
            row = json.loads(out.stdout)
            assert (out.returncode == 0) == success, (name, row)
            assert row["success"] == success, (name, row)
            assert not out.stderr, (name, "unexpected stderr")
            checks.append(name)
            return row.get("data", row.get("error"))

        def ipc(frame):
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(10)
                stream.connect(str(registry / "daemon.sock"))
                stream.sendall((json.dumps(frame) + "\n").encode())
                with stream.makefile("rb") as reader:
                    return json.loads(reader.readline(1_048_577))

        # Local registration only: these identities/sessions never belong to real wallets.
        for name in ("w1", "w2", "w3", "w4"):
            wallet = root / name
            wallet.mkdir(mode=0o700)
            key = root / (name + ".identity")
            key.write_text("synthetic-smoke-only-" + name)
            key.chmod(0o600)
            cli("add-" + name, ["wallets", "add", name, "--wallet-dir", str(wallet),
                                   "--instance-id-file", str(key)])
        assert len(cli("list", ["wallets", "list"])["wallets"]) == 4
        WARM = "--wa" "rm"  # split: the retired flag must not exist, not even as help text
        # The single-wallet entry points are retired: they are unknown arguments now, so
        # nothing cold-executes and no selector can override the registry.
        for label, retired in (
            ("wallet-dir", ["--wallet-dir", str(root / "w1"), "wallet", "status"]),
            ("instance-id", ["--instance-id", "synthetic-smoke-only-w1", "wallet", "status"]),
            ("instance-id-file", ["--instance-id-file", str(root / "w1.identity"),
                                  "wallet", "status"]),
            ("warm", [WARM, "wallet", "status"]),
            ("warm-with-wallet", ["--wallet", "w1", WARM, "wallet", "status"]),
            ("top-level-serve", ["serve"]),
        ):
            cli("retired-" + label, retired, False)
        daemon = subprocess.Popen(base + ["wallets", "serve"], env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            end = time.monotonic() + 10
            while not (registry / "daemon.sock").exists():
                assert daemon.poll() is None, "daemon exited before startup"
                assert time.monotonic() < end, "daemon startup timed out"
                time.sleep(0.01)
            assert (registry / "daemon.sock").stat().st_mode & 0o777 == 0o600
            assert cli("status", ["wallets", "status"])["pid"] == daemon.pid
            for name in ("w1", "w2", "w3", "w4"):
                cli("empty-wallet-" + name, ["--wallet", name, "wallet", "status"], False)
                reply = ipc({"op": "run", "name": name, "argv": ["wallet", "status"]})
                assert reply["success"] is False
                checks.append("empty-wallet-ipc-" + name)
            for operation in ("prepare", "trigger", "disarm", "native"):
                reply = ipc({"op": operation, "id": "retired", "name": "w1"})
                assert reply["success"] is False
                assert reply["error"]["message"] == "invalid IPC request"
                checks.append("retired-" + operation)

            # No RPC URL, no credentials and no execution. Arm without consent must reject.
            def action(name):
                return {"wallet": name, "argv": ["contract-call", "fire", "--binanceChainId", "56",
                        "--from", "0x1111111111111111111111111111111111111111", "--to",
                        "0x2222222222222222222222222222222222222222", "--value", "0"]}

            config = {"id": "offline-smoke", "chain_id": "56", "steps": [
                {"task_id": "task" + str(i), "trigger": {"kind": "immediate"},
                 "valid_for_ms": 60000, "completion": "submitted",
                 "actions": [action(w) for w in names]}
                for i, names in enumerate((("w1", "w2"), ("w2", "w3"), ("w3", "w4")), 1)]}
            path = root / "pipeline.json"
            path.write_text(json.dumps(config))
            path.chmod(0o600)
            assert cli("load", ["pipeline", "load", "--config", str(path)])["status"] == "LOADED"
            state = cli("task-alias", ["task", "status", "offline-smoke"])
            assert state["snapshot"]["steps"][1]["wallets"] == ["w2", "w3"]
            cli("unconfirmed-arm", ["pipeline", "arm", "offline-smoke"], False)
            cli("wallet-scoped-task-rejected", ["--wallet", "w1", "task", "arm", "offline-smoke", "--confirm"], False)
            assert cli("cancel", ["pipeline", "cancel", "offline-smoke"])["status"] == "CANCELLED"
            cli("cancelled-resume-rejected", ["pipeline", "resume", "offline-smoke", "--confirm"], False)
            assert (registry / "pipeline-offline-smoke.json").stat().st_mode & 0o777 == 0o600
            assert not list(root.rglob("session.json")), "test unexpectedly created a session"
            checks.append("no-sessions-created")
            cli("stop", ["wallets", "stop"])
            stdout, stderr = daemon.communicate(timeout=10)
            assert daemon.returncode == 0 and not stderr
            lines = stdout.splitlines()
            assert len(lines) == 1 and json.loads(lines[0])["data"]["status"] == "stopped"
            assert not (registry / "daemon.sock").exists()
            checks.append("clean-daemon-exit")
        finally:
            if daemon.poll() is None:
                daemon.kill()
                daemon.communicate(timeout=10)
    return {"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "checks": checks, "passed": len(checks), "production_verified": False,
            "real_wallets_opened": False, "transactions_submitted": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = smoke(args.binary.resolve(strict=True))
    if args.output:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
