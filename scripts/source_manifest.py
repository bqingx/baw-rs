#!/usr/bin/env python3
"""Write verification/source-sha256.json: sha256 of every tracked source file.

Covers what the gates actually compiled: manifest, lockfile, src/**, tests/**.
Excludes verification/ itself (it is the output, not an input).
"""
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)

targets = ["Cargo.toml", "Cargo.lock"]
for d in ("src", "tests"):
    for name in sorted(os.listdir(d)):
        if name.endswith(".rs"):
            targets.append(os.path.join(d, name))

manifest = {p: hashlib.sha256(open(p, "rb").read()).hexdigest() for p in targets}

# Provenance: the commit these hashes describe, when the tree is a git repo.
try:
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=no", "--", *targets],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
except (OSError, subprocess.CalledProcessError):
    commit, dirty = "not-a-git-repo", ""

out = {
    "_note": "Regenerate with scripts/verify.sh. Hashes describe the tree at "
             "'commit'; dirty_source is true when tracked files differ from it.",
    "commit": commit,
    "dirty_source": bool(dirty),
    "files": manifest,
}
os.makedirs("verification", exist_ok=True)
with open("verification/source-sha256.json", "w") as fh:
    json.dump(out, fh, indent=2)
    fh.write("\n")
print(f"source-sha256.json: {len(manifest)} files, commit {commit[:12]}, dirty={bool(dirty)}")
sys.exit(0)
