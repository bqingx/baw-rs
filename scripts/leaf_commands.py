#!/usr/bin/env python3
"""Enumerate the leaf commands a built binary actually exposes.

Docs drift; `--help` does not. Writes verification/leaf-commands.txt and prints a
summary, so any claim of the form "N commands" can be checked instead of asserted.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(
    os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target")), "release", "baw-rs"
)

TOP_LEVEL_REJECT = {"help"}


def leaves(binary, path):
    """Return leaf argv paths below `path`."""
    out = subprocess.run(
        [binary, *path, "--help"], capture_output=True, text=True
    )
    text = out.stdout
    subs = []
    if "Commands:" in text:
        block = text.split("Commands:", 1)[1].split("Options:", 1)[0]
        for line in block.splitlines():
            line = line.rstrip()
            if not line.startswith("  ") or line.strip().startswith("-"):
                continue
            name = line.strip().split()[0]
            if name and name[0].islower():
                subs.append(name)
    subs = [s for s in subs if s not in TOP_LEVEL_REJECT]
    if not subs:
        return [" ".join(path)]
    found = []
    for s in subs:
        found.extend(leaves(binary, [*path, s]))
    return found


def main():
    if not os.path.exists(BIN):
        print(f"missing binary: {BIN}", file=sys.stderr)
        return 1
    top = leaves(BIN, [])
    # leaves() already excludes "help" at every level; drop the bare binary entry.
    top = [c for c in top if c]
    os.makedirs(os.path.join(ROOT, "verification"), exist_ok=True)
    with open(os.path.join(ROOT, "verification", "leaf-commands.txt"), "w") as fh:
        fh.write(f"# {BIN} --version: {subprocess.run([BIN, '--version'], capture_output=True, text=True).stdout.strip()}\n")
        for c in top:
            fh.write(c + "\n")
        fh.write(f"# total: {len(top)}\n")
    print(f"leaf-commands.txt: {len(top)} commands")
    return 0


if __name__ == "__main__":
    sys.exit(main())
