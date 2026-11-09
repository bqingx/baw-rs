#!/usr/bin/env python3
"""Inventory actual CLI leaves, including the separate openfour parser, from help only."""
import os
from pathlib import Path
import sys
from build_skill_reference import collect

ROOT = Path(__file__).resolve().parents[1]


def main():
    binary = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target"))) / "release" / "baw-rs"
    if not binary.is_absolute():
        binary = ROOT / binary
    if not binary.is_file():
        print(f"missing binary: {binary}", file=sys.stderr)
        return 1
    _, commands, version = collect(binary)
    output = ROOT / "verification" / "leaf-commands.txt"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(f"# {binary} --version: {version}\n" + "\n".join(commands)
                      + f"\n# total: {len(commands)}\n")
    print(f"leaf-commands.txt: {len(commands)} canonical commands (aliases not duplicated)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
