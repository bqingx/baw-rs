#!/usr/bin/env python3
"""Offline gates + real release daemon smoke. Stdlib only; never opens real wallets."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "verification" / "multiwallet"
ENV = os.environ.copy()
ENV["PATH"] = str(Path.home() / ".cargo/bin") + os.pathsep + ENV.get("PATH", "")
SCRATCH = Path(ENV.get("TMPDIR", str(Path.home() / ".hermes/cache/scratch")))
SCRATCH.mkdir(parents=True, exist_ok=True)
ENV["TMPDIR"] = str(SCRATCH)
ENV.pop("BINANCE_BAW_DIR", None)
ENV.pop("BINANCE_INSTANCE_ID", None)
TARGET = Path(ENV.get("CARGO_TARGET_DIR", str(ROOT / "target"))).resolve()
OUT.mkdir(parents=True, exist_ok=True)
results = []


def command(name, argv, expected=0):
    start = time.monotonic()
    with (OUT / (name + ".log")).open("wb") as output:
        p = subprocess.run(argv, cwd=ROOT, env=ENV, stdout=output, stderr=subprocess.STDOUT, check=False)
    results.append({"name": name, "argv": list(map(str, argv)), "exit_code": p.returncode,
                    "expected_exit_code": expected, "seconds": round(time.monotonic() - start, 3)})
    (OUT / "exit-codes.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{name}: exit={p.returncode}", flush=True)
    if p.returncode != expected:
        raise RuntimeError(f"{name} failed; see full log")
    return (OUT / (name + ".log")).read_text()


def smoke(binary):
    with tempfile.TemporaryDirectory(prefix="baw-multi-", dir=SCRATCH) as temp:
        root = Path(temp)
        registry = root / "registry"
        common = [str(binary), "--registry", str(registry), "wallets"]
        for name in ("alice", "bob"):
            wallet = root / name
            wallet.mkdir(mode=0o700)
            key = root / (name + ".identity")
            key.write_text("synthetic-" + name)
            key.chmod(0o600)
            command("register-" + name, common + ["register", name, "--wallet-dir", str(wallet),
                    "--instance-id-file", str(key)])
        listing = json.loads(command("list", common + ["list"]))
        assert [w["name"] for w in listing["data"]["wallets"]] == ["alice", "bob"]
        with (OUT / "daemon.stdout.log").open("wb") as stdout, (OUT / "daemon.stderr.log").open("wb") as stderr:
            process = subprocess.Popen(common + ["serve"], cwd=ROOT, env=ENV, stdout=stdout, stderr=stderr)
            try:
                deadline = time.monotonic() + 10
                while not (registry / "daemon.sock").exists():
                    assert process.poll() is None, "daemon exited before startup"
                    assert time.monotonic() < deadline, "daemon startup timed out"
                    time.sleep(0.01)
                assert (registry / "daemon.sock").stat().st_mode & 0o777 == 0o600
                status = json.loads(command("status", common + ["status"]))
                assert status["data"]["pid"] == process.pid
                assert status["data"]["wallets"] == ["alice", "bob"]
                children = Path(f"/proc/{process.pid}/task/{process.pid}/children")
                if children.exists():
                    assert not children.read_text().strip(), "daemon spawned child processes"
                command("unknown-wallet", common[:3] + ["--wallet", "missing", "wallet", "status"], 1)
                for name in ("alice", "bob"):
                    # Empty directory -> fail before any HTTP, never silently choose another wallet.
                    command("empty-" + name, common[:3] + ["--wallet", name, "wallet", "status"], 1)
                    assert not (root / name / "session.json").exists()
                command("selector-rejected", common[:3] + ["--wallet", "alice",
                        "--instance-id=forbidden", "wallet", "status"], 1)
                command("stream-rejected", common[:3] + ["--wallet", "alice", "tracker", "ws", "--following"], 1)
                command("stop", common + ["stop"])
                rc = process.wait(timeout=10)
                results.append({"name": "daemon", "argv": common + ["serve"], "exit_code": rc, "expected_exit_code": 0})
                assert rc == 0
                assert not (registry / "daemon.sock").exists()
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        # No business event/request or credentials may leak to daemon output.
        output = (OUT / "daemon.stdout.log").read_text().splitlines()
        assert len(output) == 1 and json.loads(output[0])["data"]["status"] == "stopped"
        assert not (OUT / "daemon.stderr.log").read_text()


def main():
    command("fmt", ["cargo", "fmt", "--all", "--", "--check"])
    command("clippy", ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    tests = command("cargo-test", ["cargo", "test", "--all-targets"])
    counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", tests)
    assert counts
    command("release-build", ["cargo", "build", "--release"])
    binary = TARGET / "release" / "baw-rs"
    command("release-version", [str(binary), "--version"])
    command("wallets-help", [str(binary), "wallets", "--help"])
    smoke(binary)
    manifest = {}
    for folder in ("src", "tests", "scripts", "docs"):
        for path in sorted((ROOT / folder).rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                manifest[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    for name in ("Cargo.toml", "Cargo.lock", "README.md"):
        manifest[name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    git = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    report = {"base_commit": git, "dirty_source": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
              "production_verified": False, "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "tests_passed": sum(int(x[0]) for x in counts), "tests_failed": sum(int(x[1]) for x in counts),
              "tests_ignored": sum(int(x[2]) for x in counts), "source_sha256": manifest}
    (OUT / "source-and-release.json").write_text(json.dumps(report, indent=2) + "\n")
    (OUT / "exit-codes.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "source_sha256"}, indent=2))


if __name__ == "__main__":
    main()
