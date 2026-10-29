#!/bin/sh
# Regenerate every gate artefact in verification/ from the current source tree.
# Usage: CARGO_TARGET_DIR=/path/to/target sh scripts/verify.sh
# Requires: cargo, rustfmt, clippy (rustup component add rustfmt clippy)
set -eu
cd "$(dirname "$0")/.."
T="${CARGO_TARGET_DIR:-target}"
export CARGO_TARGET_DIR="$T"
mkdir -p verification

echo "[1/5] fmt"
cargo fmt --check >verification/fmt.log 2>&1 && echo "exit=0" >>verification/fmt.log || { echo "exit=$?" >>verification/fmt.log; exit 1; }

echo "[2/5] clippy --all-targets -D warnings"
cargo clippy --all-targets -- -D warnings >verification/clippy.log 2>&1 && echo "exit=0" >>verification/clippy.log || { echo "exit=$?" >>verification/clippy.log; exit 1; }

echo "[3/5] test"
cargo test >verification/cargo-test.log 2>&1 && echo "exit=0" >>verification/cargo-test.log || { echo "exit=$?" >>verification/cargo-test.log; exit 1; }

echo "[4/5] build --release"
cargo build --release >verification/release-build.log 2>&1 && echo "exit=0" >>verification/release-build.log || { echo "exit=$?" >>verification/release-build.log; exit 1; }

echo "[5/6] source manifest + release identity"
python3 scripts/source_manifest.py

BIN="$CARGO_TARGET_DIR/release/baw-rs"
printf 'binary: %s\nsha256: %s\nbytes: %s\nversion: %s\ntoolchain: %s\n' \
  "$BIN" "$(sha256sum "$BIN" | cut -d' ' -f1)" "$(stat -c%s "$BIN")" \
  "$("$BIN" --version)" "$(rustc --version)" >verification/release-identity.txt
cat verification/release-identity.txt

echo "[6/6] leaf command inventory"
python3 scripts/leaf_commands.py
